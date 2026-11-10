// SPDX-License-Identifier: MIT

//! Structured, trace-correlated logging as purerest's `Logging` does it,
//! written as logstash-logback-encoder's `LogstashEncoder` writes it in the
//! Docker images (`logback-docker.xml`; RUST-3.4): one JSON object per line,
//! `@timestamp` first, so Filebeat decodes it (gluon filebeat.yml).

use std::fmt::Write as _;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use opentelemetry::Context;
use opentelemetry::trace::TraceContextExt;
use time::{OffsetDateTime, UtcOffset};

/// A context field: name and value, written in the order given.
pub type Field<'a> = (&'a str, &'a str);

#[derive(Debug, Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    const fn name(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    /// logback's `Level.toInt()`.
    const fn value(self) -> u32 {
        match self {
            Self::Info => 20000,
            Self::Warn => 30000,
            Self::Error => 40000,
        }
    }
}

/// A named logger (purerest: `Slf4jLogger.fromName(serviceName)`) that stamps
/// each line with the current span's `trace_id` and `span_id`.
#[derive(Clone)]
pub struct Logger {
    name: Arc<str>,
    out: Arc<Mutex<dyn Write + Send>>,
}

impl std::fmt::Debug for Logger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Logger")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Logger {
    /// Lines go to stdout, as the console appender writes them.
    #[must_use]
    pub fn stdout(name: &str) -> Self {
        Self {
            name: name.into(),
            out: Arc::new(Mutex::new(std::io::stdout())),
        }
    }

    /// Lines go to `writer` (tests pass a buffer).
    #[must_use]
    pub fn to_writer<W: Write + Send + 'static>(name: &str, writer: Arc<Mutex<W>>) -> Self {
        Self {
            name: name.into(),
            out: writer,
        }
    }

    /// An INFO line.
    pub fn info(&self, fields: &[Field<'_>], message: &str) {
        self.write(Level::Info, fields, message);
    }

    /// A WARN line.
    pub fn warn(&self, fields: &[Field<'_>], message: &str) {
        self.write(Level::Warn, fields, message);
    }

    /// An ERROR line.
    pub fn error(&self, fields: &[Field<'_>], message: &str) {
        self.write(Level::Error, fields, message);
    }

    fn write(&self, level: Level, fields: &[Field<'_>], message: &str) {
        let thread = std::thread::current();
        let mut line = String::with_capacity(256);
        line.push('{');
        push(
            &mut line,
            "@timestamp",
            &logstash_timestamp(OffsetDateTime::now_utc()),
        );
        push(&mut line, "@version", "1");
        push(&mut line, "message", message);
        push(&mut line, "logger_name", &self.name);
        push(&mut line, "thread_name", thread.name().unwrap_or("main"));
        push(&mut line, "level", level.name());
        let _ = write!(line, ",\"level_value\":{}", level.value());
        for (name, value) in fields {
            push(&mut line, name, value);
        }
        let cx = Context::current();
        let span = cx.span();
        let context = span.span_context();
        if context.is_valid() {
            push(&mut line, "trace_id", &context.trace_id().to_string());
            push(&mut line, "span_id", &context.span_id().to_string());
        }
        line.push_str("}\n");
        let mut out = self.out.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
    }
}

fn push(line: &mut String, name: &str, value: &str) {
    if !line.ends_with('{') {
        line.push(',');
    }
    // Strings always serialize.
    line.push_str(&serde_json::to_string(name).unwrap_or_default());
    line.push(':');
    line.push_str(&serde_json::to_string(value).unwrap_or_default());
}

/// `@timestamp` as `DateTimeFormatter.ISO_OFFSET_DATE_TIME` prints an instant
/// from Java's clock in a UTC container: microseconds at most, the fraction's
/// trailing zeros dropped (none at all on a whole second), then `Z`.
#[must_use]
pub fn logstash_timestamp(at: OffsetDateTime) -> String {
    let utc = at.to_offset(UtcOffset::UTC);
    let mut text = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
    );
    let micros = utc.microsecond();
    if micros > 0 {
        let fraction = format!("{micros:06}");
        let _ = write!(text, ".{}", fraction.trim_end_matches('0'));
    }
    text.push('Z');
    text
}
