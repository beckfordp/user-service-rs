// SPDX-License-Identifier: MIT

//! Resilience as purerest (HTTP clients) and purekafka (Kafka calls) give it:
//! bounded retry with exponential backoff and a count-based circuit breaker
//! (resilience4j's, as both configure it; RUST-6.3).

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::metrics::{Counter, Gauge, Metrics};

/// `maxRetries` retries (so `max_retries + 1` attempts), the delay doubling
/// from `base_delay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryConfig {
    /// Retries, not attempts.
    pub max_retries: u32,
    /// The first retry's delay; each later one doubles it.
    pub base_delay: Duration,
}

/// `failure_threshold` calls in a row failing open the breaker for
/// `reset_timeout`, then one trial call decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitBreakerConfig {
    /// The count-based window's size, all of which must fail.
    pub failure_threshold: u32,
    /// How long the breaker stays open before a half-open trial.
    pub reset_timeout: Duration,
}

/// Bounded retry with exponential backoff (purerest/purekafka `Retry`),
/// counting attempts in `<prefix>.retry.attempts{outcome}`.
#[derive(Debug, Clone)]
pub struct Retry {
    config: RetryConfig,
    attempts: Counter,
}

impl Retry {
    /// The policy, its counter in `metrics` named after `prefix` (`purerest`,
    /// `purekafka`).
    #[must_use]
    pub fn new(config: RetryConfig, metrics: &Metrics, prefix: &str) -> Self {
        Self {
            config,
            attempts: metrics.counter(&format!("{prefix}.retry.attempts")),
        }
    }

    /// Runs `attempt` until `worth_retrying` says its result isn't, or the
    /// retries run out (then the last result is returned). `failed` says
    /// whether the final result counts as `exhausted` or `succeeded`.
    ///
    /// # Errors
    /// The last attempt's error, if it failed.
    pub async fn run<T, E, F, Fut>(
        &self,
        mut attempt: F,
        worth_retrying: impl Fn(&Result<T, E>) -> bool,
        failed: impl Fn(&Result<T, E>) -> bool,
    ) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            let result = attempt().await;
            if attempts <= self.config.max_retries && worth_retrying(&result) {
                let doubling = 2_u32.saturating_pow(attempts - 1);
                tokio::time::sleep(self.config.base_delay.saturating_mul(doubling)).await;
                continue;
            }
            if attempts > 1 {
                self.attempts
                    .add(u64::from(attempts - 1), &[("outcome", "retried")]);
            }
            let outcome = if failed(&result) {
                "exhausted"
            } else {
                "succeeded"
            };
            self.attempts.add(1, &[("outcome", outcome)]);
            return result;
        }
    }
}

/// The breaker's states, named as resilience4j names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed,
    Open,
    HalfOpen,
}

impl State {
    const ALL: [Self; 3] = [Self::Closed, Self::Open, Self::HalfOpen];

    const fn name(self) -> &'static str {
        match self {
            Self::Closed => "CLOSED",
            Self::Open => "OPEN",
            Self::HalfOpen => "HALF_OPEN",
        }
    }
}

#[derive(Debug)]
struct Breaker {
    state: State,
    window: VecDeque<bool>,
    opened_at: Option<Instant>,
    trial_in_flight: bool,
}

/// resilience4j's circuit breaker as purerest and purekafka configure it: a
/// count-based window of `failure_threshold` calls, opening only when all of
/// them failed; open for `reset_timeout`, then one half-open trial call.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    inner: Arc<Mutex<Breaker>>,
    state_gauge: Gauge,
    transitions: Counter,
    rejected: Counter,
}

impl CircuitBreaker {
    /// A closed breaker, its metrics in `metrics` named after `prefix`.
    #[must_use]
    pub fn new(config: CircuitBreakerConfig, metrics: &Metrics, prefix: &str) -> Self {
        Self {
            config,
            inner: Arc::new(Mutex::new(Breaker {
                state: State::Closed,
                window: VecDeque::new(),
                opened_at: None,
                trial_in_flight: false,
            })),
            state_gauge: metrics.gauge(&format!("{prefix}.circuit_breaker.state")),
            transitions: metrics.counter(&format!("{prefix}.circuit_breaker.state_transitions")),
            rejected: metrics.counter(&format!("{prefix}.circuit_breaker.calls_rejected")),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Breaker> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record_state(&self, state: State) {
        for candidate in State::ALL {
            let value = if candidate == state { 1.0 } else { 0.0 };
            self.state_gauge.set(value, &[("state", candidate.name())]);
        }
    }

    fn transition(&self, breaker: &mut Breaker, to: State) {
        if breaker.state != to {
            self.transitions.add(
                1,
                &[
                    ("from_state", breaker.state.name()),
                    ("to_state", to.name()),
                ],
            );
            breaker.state = to;
        }
    }

    /// Whether a call may go ahead now (a rejection is counted).
    #[must_use]
    pub fn acquire(&self) -> bool {
        let mut breaker = self.lock();
        if breaker.state == State::Open
            && breaker
                .opened_at
                .is_some_and(|at| at.elapsed() >= self.config.reset_timeout)
        {
            self.transition(&mut breaker, State::HalfOpen);
            breaker.trial_in_flight = false;
        }
        self.record_state(breaker.state);
        let permitted = match breaker.state {
            State::Closed => true,
            State::Open => false,
            State::HalfOpen => !std::mem::replace(&mut breaker.trial_in_flight, true),
        };
        if !permitted {
            self.rejected.add(1, &[]);
        }
        permitted
    }

    /// The outcome of a call `acquire` let through.
    pub fn record(&self, success: bool) {
        let mut breaker = self.lock();
        match breaker.state {
            State::HalfOpen => {
                breaker.trial_in_flight = false;
                breaker.window.clear();
                if success {
                    self.transition(&mut breaker, State::Closed);
                } else {
                    breaker.opened_at = Some(Instant::now());
                    self.transition(&mut breaker, State::Open);
                }
            }
            State::Closed => {
                breaker.window.push_back(success);
                let size = usize::try_from(self.config.failure_threshold.max(1)).unwrap_or(1);
                while breaker.window.len() > size {
                    breaker.window.pop_front();
                }
                if breaker.window.len() == size && breaker.window.iter().all(|ok| !ok) {
                    breaker.window.clear();
                    breaker.opened_at = Some(Instant::now());
                    self.transition(&mut breaker, State::Open);
                }
            }
            State::Open => {}
        }
        self.record_state(breaker.state);
    }
}
