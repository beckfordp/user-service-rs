// SPDX-License-Identifier: MIT

//! Shared by the integration tests: a throwaway PostgreSQL 16 cluster from
//! the local binaries (no Docker in the sandbox) and the service binary.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod otlp;

use std::collections::HashMap;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

/// A port nothing listens on right now.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// The PostgreSQL binaries: `CONTRACT_PG_BIN`, else the directory of `pg_ctl` on the PATH.
fn pg_bin() -> PathBuf {
    if let Ok(explicit) = std::env::var("CONTRACT_PG_BIN") {
        return PathBuf::from(explicit);
    }
    let path = std::env::var("PATH").unwrap_or_default();
    let pg_ctl = std::env::split_paths(&path)
        .map(|dir| dir.join("pg_ctl"))
        .find(|candidate| candidate.is_file())
        .expect("pg_ctl on the PATH, or CONTRACT_PG_BIN");
    pg_ctl
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// A PostgreSQL cluster for one test: the `user` superuser with password
/// `user` over TCP, database `user`, as gluon-local's user-postgres.
pub struct Postgres {
    bin: PathBuf,
    data: tempfile::TempDir,
    socket: tempfile::TempDir,
    pub port: u16,
}

impl Postgres {
    pub fn start() -> Self {
        let bin = pg_bin();
        let data = tempfile::tempdir().unwrap();
        let socket = tempfile::Builder::new()
            .prefix("rs-pg-")
            .tempdir_in("/tmp")
            .unwrap();
        let password = data.path().join("password");
        std::fs::write(&password, "user\n").unwrap();
        run(
            &bin.join("initdb"),
            &[
                "-D",
                data.path().join("db").to_str().unwrap(),
                "-U",
                "user",
                &format!("--pwfile={}", password.display()),
                "--auth-local=trust",
                "--auth-host=scram-sha-256",
                "--locale=C.UTF-8",
            ],
        );
        let postgres = Self {
            bin,
            data,
            socket,
            port: free_port(),
        };
        postgres.start_again();
        run(
            &postgres.bin.join("createdb"),
            &[
                "-h",
                postgres.socket_dir(),
                "-p",
                &postgres.port.to_string(),
                "-U",
                "user",
                "user",
            ],
        );
        postgres
    }

    fn socket_dir(&self) -> &str {
        self.socket.path().to_str().unwrap()
    }

    pub fn stop(&self) {
        run(
            &self.bin.join("pg_ctl"),
            &[
                "stop",
                "-D",
                self.data.path().join("db").to_str().unwrap(),
                "-m",
                "fast",
                "-w",
            ],
        );
    }

    pub fn start_again(&self) {
        let options = format!(
            "-c listen_addresses=127.0.0.1 -c port={} -c unix_socket_directories={} -c wal_level=logical",
            self.port,
            self.socket_dir()
        );
        run(
            &self.bin.join("pg_ctl"),
            &[
                "start",
                "-D",
                self.data.path().join("db").to_str().unwrap(),
                "-l",
                self.data.path().join("log").to_str().unwrap(),
                "-w",
                "-o",
                &options,
            ],
        );
    }

    /// The service's `POSTGRES_*` variables for this cluster.
    pub fn env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("POSTGRES_HOST", "127.0.0.1".to_owned()),
            ("POSTGRES_PORT", self.port.to_string()),
            ("POSTGRES_DB", "user".to_owned()),
            ("POSTGRES_USER", "user".to_owned()),
            ("POSTGRES_PASSWORD", "user".to_owned()),
        ]
    }

    /// One SQL statement through psql; rows as `|`-separated lines.
    pub fn query(&self, sql: &str) -> String {
        let output = Command::new(self.bin.join("psql"))
            .args([
                "-X",
                "-tA",
                "-h",
                self.socket_dir(),
                "-p",
                &self.port.to_string(),
            ])
            .args(["-U", "user", "-d", "user", "-c", sql])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Drop for Postgres {
    fn drop(&mut self) {
        let _ = Command::new(self.bin.join("pg_ctl"))
            .args([
                "stop",
                "-D",
                self.data.path().join("db").to_str().unwrap(),
                "-m",
                "immediate",
            ])
            .output();
    }
}

fn run(program: &Path, args: &[&str]) {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{} {args:?}: {}",
        program.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The service binary, started with exactly `env` (plus `PATH` and
/// cargo-llvm-cov's `LLVM_PROFILE_FILE`); stopped with SIGTERM on drop.
pub struct Service {
    pub child: Child,
    pub port: u16,
    pub metrics_port: u16,
    stdout: Output,
    stderr: Output,
}

/// One of the child's output streams, drained as it writes so it never
/// blocks on a full pipe.
struct Output {
    text: Arc<Mutex<String>>,
    reader: Option<JoinHandle<()>>,
}

impl Output {
    fn drain(mut stream: impl Read + Send + 'static) -> Self {
        let text = Arc::new(Mutex::new(String::new()));
        let sink = text.clone();
        let reader = std::thread::spawn(move || {
            let mut chunk = [0_u8; 8192];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&chunk[..read]));
            }
        });
        Self {
            text,
            reader: Some(reader),
        }
    }

    /// What was written so far; everything, once `finished` (the child exited).
    fn text(&mut self, finished: bool) -> String {
        if finished && let Some(reader) = self.reader.take() {
            reader.join().unwrap();
        }
        self.text.lock().unwrap().clone()
    }
}

impl Service {
    /// Starts it on fresh ports; does not wait.
    pub fn spawn(extra: &[(&str, String)]) -> Self {
        let port = free_port();
        let metrics_port = free_port();
        let mut env: HashMap<&str, String> = HashMap::from([
            ("USER_SERVICE_PORT", port.to_string()),
            ("USER_SERVICE_METRICS_PORT", metrics_port.to_string()),
        ]);
        env.extend(extra.iter().map(|(key, value)| (*key, value.clone())));
        let child = Command::new(env!("CARGO_BIN_EXE_user-service"))
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .envs(std::env::vars().filter(|(key, _)| key == "LLVM_PROFILE_FILE"))
            .envs(&env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = child;
        let stdout = Output::drain(child.stdout.take().unwrap());
        let stderr = Output::drain(child.stderr.take().unwrap());
        Self {
            child,
            port,
            metrics_port,
            stdout,
            stderr,
        }
    }

    /// Starts it and waits until `/health` answers.
    pub fn start(extra: &[(&str, String)]) -> Self {
        let mut service = Self::spawn(extra);
        let deadline = Instant::now() + Duration::from_secs(30);
        while service.get("/health").is_none() {
            assert!(
                service.child.try_wait().unwrap().is_none(),
                "exited before serving"
            );
            assert!(Instant::now() < deadline, "never served /health");
            sleep(Duration::from_millis(50));
        }
        service
    }

    /// `GET path`: status, Content-Type and body, or None if nothing answered.
    pub fn get(&self, path: &str) -> Option<(u16, Option<String>, String)> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let mut response = agent
            .get(format!("http://127.0.0.1:{}{path}", self.port))
            .call()
            .ok()?;
        let content_type = response
            .headers()
            .get("content-type")
            .map(|value| value.to_str().unwrap().to_owned());
        let body = response.body_mut().read_to_string().unwrap();
        Some((response.status().as_u16(), content_type, body))
    }

    /// `POST path` with `body` and an optional `Content-Type`: status, Content-Type, body.
    pub fn post(
        &self,
        path: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> (u16, Option<String>, String) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let mut request = agent.post(format!("http://127.0.0.1:{}{path}", self.port));
        if let Some(content_type) = content_type {
            request = request.header("Content-Type", content_type);
        }
        let mut response = request.send(body).unwrap();
        let received = response
            .headers()
            .get("content-type")
            .map(|value| value.to_str().unwrap().to_owned());
        let text = response.body_mut().read_to_string().unwrap();
        (response.status().as_u16(), received, text)
    }

    /// `POST path` with a JSON body.
    pub fn post_json(&self, path: &str, json: &str) -> (u16, Option<String>, String) {
        self.post(path, json.as_bytes(), Some("application/json"))
    }

    /// Any request without a body: status, all headers (lower-case names), body.
    pub fn request(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
    ) -> (u16, Vec<(String, String)>, String) {
        self.request_with_body(method, path, headers, "")
    }

    /// Any request: method, path, headers, body → status, all headers (lower-case names), body.
    /// Redirects are returned, not followed.
    pub fn request_with_body(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, Vec<(String, String)>, String) {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .allow_non_standard_methods(true)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let mut builder = ureq::http::Request::builder()
            .method(method)
            .uri(format!("http://127.0.0.1:{}{path}", self.port));
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut response = agent.run(builder.body(body.to_owned()).unwrap()).unwrap();
        let received = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), value.to_str().unwrap().to_owned()))
            .collect();
        let text = if method == "HEAD" {
            String::new()
        } else {
            response.body_mut().read_to_string().unwrap()
        };
        (response.status().as_u16(), received, text)
    }

    /// Waits for the process to exit by itself.
    pub fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "did not exit");
            sleep(Duration::from_millis(20));
        }
    }

    /// Sends SIGTERM.
    pub fn terminate(&self) {
        let status = Command::new("kill")
            .args(["-TERM", &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
    }

    /// What it has written to stderr so far (all of it once it has exited).
    pub fn stderr(&mut self) -> String {
        let exited = self.child.try_wait().unwrap().is_some();
        self.stderr.text(exited)
    }

    /// What it has written to stdout so far (all of it once it has exited).
    pub fn stdout(&mut self) -> String {
        let exited = self.child.try_wait().unwrap().is_some();
        self.stdout.text(exited)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            self.terminate();
            let _ = self.wait_exit();
        }
    }
}
