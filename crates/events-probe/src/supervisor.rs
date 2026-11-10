// SPDX-License-Identifier: MIT

//! purekafka's `StreamSupervisor`: a failed consumer stream is rebuilt from
//! scratch after a delay that doubles up to a cap, forever (a dead consumer
//! needing a manual restart is what it exists to prevent), each restart
//! counted in `purekafka.stream.restarts`.

use std::future::Future;
use std::time::Duration;

use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use tokio::sync::watch;

/// The next delay: doubled, capped at `max`.
#[must_use]
pub fn next_delay(current: Duration, max: Duration) -> Duration {
    current.saturating_mul(2).min(max)
}

fn millis(delay: Duration) -> String {
    format!("{}ms", delay.as_millis())
}

/// Runs `build` until it returns `Ok` (a clean stop) or `stop` is set;
/// after each error, waits (base, doubling to `max`) and builds again.
pub async fn supervise<F, Fut>(
    base: Duration,
    max: Duration,
    metrics: &Metrics,
    logger: &Logger,
    mut stop: watch::Receiver<bool>,
    mut build: F,
) where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let restarts = metrics.counter("purekafka.stream.restarts");
    let mut delay = base;
    let mut attempt: u32 = 1;
    loop {
        let Err(error) = build().await else {
            return;
        };
        if *stop.borrow() {
            return;
        }
        logger.warn(
            &[("error", &error)],
            &format!(
                "Consumer stream failed (attempt {attempt}), restarting after {}",
                millis(delay)
            ),
        );
        restarts.add(1, &[]);
        tokio::select! {
            _ = stop.wait_for(|stop| *stop) => return,
            () = tokio::time::sleep(delay) => {}
        }
        delay = next_delay(delay, max);
        attempt += 1;
    }
}
