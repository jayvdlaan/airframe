//! Schedule description types: [`Strategy`], [`RetryPolicy`], and [`Schedule`].

use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Strategy {
    Once(Duration),
    FixedRate(Duration),
    FixedDelay(Duration),
}

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff: Duration,
}

#[derive(Clone, Debug)]
pub struct Schedule {
    pub strategy: Strategy,
    pub max_runs: Option<u32>,
    pub timeout: Option<Duration>,
    pub retry: Option<RetryPolicy>,
    /// Maximum number of concurrent executions for this job (default 1)
    pub concurrency: Option<u32>,
    /// Optional jitter to apply before each run for FixedRate/FixedDelay schedules
    pub jitter: Option<Duration>,
}
