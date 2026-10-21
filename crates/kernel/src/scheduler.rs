//! Scheduler: Redis-backed job queue and recurring jobs built on `apalis`.
//!
//! apalis-redis owns queueing, worker leases, heartbeats and orphan
//! re-enqueueing, so there is no hand-rolled ZSET or leader election here.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use apalis::prelude::{BoxDynError, Data, TaskBuilder, TaskSink, WorkerBuilder};
use apalis_redis::RedisStorage;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LumiError, Result};

type Storage = RedisStorage<Job>;

/// Total attempts for a one-shot job, including the first run.
const MAX_ATTEMPTS: u32 = 4;
const RETRY_BASE: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

/// A hung job holds a worker slot forever; bounding it turns the stall into a
/// retryable failure while keeping the queue moving.
pub const JOB_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    #[serde(default)]
    pub payload: Option<Value>,
    /// Re-enqueue interval for recurring jobs; `None` runs once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<Duration>,
    /// Number of failed attempts already made by a one-shot job.
    #[serde(default)]
    pub attempt: u32,
}

/// Exponential backoff before the next one-shot attempt.
#[must_use]
pub fn retry_delay(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(6);
    RETRY_BASE.saturating_mul(1_u32 << shift).min(RETRY_MAX)
}

/// Whether a failed one-shot job should be re-enqueued.
#[must_use]
pub fn should_retry(err: &LumiError, attempt: u32) -> bool {
    err.is_retryable() && attempt + 1 < MAX_ATTEMPTS
}

type JobFuture = BoxFuture<'static, Result<()>>;

/// Runs one job. Errors are logged and the worker continues.
pub type JobHandler = Arc<dyn Fn(Job) -> JobFuture + Send + Sync>;

#[derive(Clone)]
struct WorkerState {
    handler: JobHandler,
    storage: Storage,
}

#[derive(Clone, Debug)]
pub struct Scheduler {
    storage: Storage,
}

impl Scheduler {
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let conn = apalis_redis::connect(redis_url)
            .await
            .map_err(|err| LumiError::Cache(err.to_string()))?;
        Ok(Self {
            storage: RedisStorage::new(conn),
        })
    }

    pub async fn enqueue(&self, job: Job) -> Result<()> {
        let mut storage = self.storage.clone();
        storage.push(job).await.map_err(sink_error)
    }

    /// Enqueue a job to run no earlier than `run_at`.
    pub async fn enqueue_delayed(&self, job: Job, run_at: SystemTime) -> Result<()> {
        let mut storage = self.storage.clone();
        storage
            .push_task(TaskBuilder::new(job).run_at_time(run_at).build())
            .await
            .map_err(sink_error)
    }

    /// Enqueue a recurring job that re-schedules itself every `every`.
    pub async fn register_interval(&self, name: &str, every: Duration) -> Result<()> {
        self.enqueue(Job {
            name: name.to_string(),
            payload: None,
            every: Some(every),
            attempt: 0,
        })
        .await
    }

    /// Run the worker until the backend ends, dispatching jobs to `handler`.
    pub async fn run(self, handler: JobHandler) {
        let state = WorkerState {
            handler,
            storage: self.storage.clone(),
        };
        let worker = WorkerBuilder::new("lumi-scheduler")
            .backend(self.storage)
            .data(state)
            .build(dispatch);
        if let Err(err) = worker.run().await {
            tracing::error!(error = %err, "scheduler worker stopped");
        }
    }
}

fn sink_error(err: impl std::fmt::Display) -> LumiError {
    LumiError::Cache(err.to_string())
}

#[tracing::instrument(name = "job.dispatch", skip(state), fields(job = %job.name, attempt = job.attempt))]
async fn dispatch(job: Job, state: Data<WorkerState>) -> std::result::Result<(), BoxDynError> {
    let Job {
        name,
        payload,
        every,
        attempt,
    } = job.clone();

    let started = Instant::now();
    let outcome = crate::retry::with_timeout("job", JOB_TIMEOUT, (state.handler)(job)).await;
    metrics::histogram!("lumi.job.duration_ms", "job" => name.clone())
        .record(started.elapsed().as_secs_f64() * 1000.0);

    if let Err(err) = outcome {
        let class = if err.is_retryable() {
            "retryable"
        } else {
            "permanent"
        };
        metrics::counter!("lumi.job.failed", "job" => name.clone(), "class" => class).increment(1);

        // Recurring jobs self-schedule below, so a short retry loop would
        // duplicate their schedule; wait for the next tick instead.
        if every.is_none() && should_retry(&err, attempt) {
            let next = Job {
                name: name.clone(),
                payload: payload.clone(),
                every: None,
                attempt: attempt + 1,
            };
            let delay = retry_delay(attempt + 1);
            let mut storage = state.storage.clone();
            storage
                .push_task(TaskBuilder::new(next).run_after(delay).build())
                .await?;
            metrics::counter!("lumi.job.retried", "job" => name.clone()).increment(1);
            tracing::warn!(
                job = %name,
                attempt = attempt + 1,
                delay_ms = delay.as_millis(),
                error = %err,
                "scheduled job failed; retrying with backoff"
            );
            return Ok(());
        }

        tracing::error!(
            job = %name,
            attempt,
            error = %err,
            "scheduled job failed permanently"
        );
    } else {
        metrics::counter!("lumi.job.completed", "job" => name.clone()).increment(1);
    }

    if let Some(every) = every {
        let next = Job {
            name,
            payload,
            every: Some(every),
            attempt: 0,
        };
        let mut storage = state.storage.clone();
        storage
            .push_task(TaskBuilder::new(next).run_after(every).build())
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MAX_ATTEMPTS, RETRY_MAX, retry_delay, should_retry};
    use crate::LumiError;
    use std::time::Duration;

    #[test]
    fn retry_delay_grows_exponentially_and_is_capped() {
        assert_eq!(retry_delay(1), Duration::from_secs(5));
        assert_eq!(retry_delay(2), Duration::from_secs(10));
        assert_eq!(retry_delay(3), Duration::from_secs(20));
        assert_eq!(retry_delay(99), RETRY_MAX);
    }

    #[test]
    fn retryable_errors_retry_until_the_attempt_budget_is_spent() {
        let err = LumiError::Timeout("redis".into());
        assert!(should_retry(&err, 0));
        assert!(should_retry(&err, MAX_ATTEMPTS - 2));
        assert!(!should_retry(&err, MAX_ATTEMPTS - 1));
    }

    #[test]
    fn permanent_errors_never_retry() {
        let err = LumiError::Invalid("bad payload".into());
        assert!(!should_retry(&err, 0));
    }

    #[tokio::test]
    async fn hung_job_times_out_and_is_retried() {
        let result: crate::Result<()> = crate::retry::with_timeout(
            "job",
            std::time::Duration::from_millis(5),
            async {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                Ok(())
            },
        )
        .await;

        let Err(err) = result else {
            panic!("expected the job to time out");
        };
        assert!(err.is_retryable());
        assert!(should_retry(&err, 0));
    }
}
