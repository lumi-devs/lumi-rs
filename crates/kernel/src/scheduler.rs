//! Scheduler: Redis-backed job queue and recurring jobs built on `apalis`.
//!
//! apalis-redis owns queueing, worker leases, heartbeats and orphan
//! re-enqueueing, so there is no hand-rolled ZSET or leader election here.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use apalis::prelude::{BoxDynError, Data, TaskBuilder, TaskSink, WorkerBuilder};
use apalis_redis::RedisStorage;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{LumiError, Result};

type Storage = RedisStorage<Job>;

/// A unit of scheduled work handed to the job handler.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub name: String,
    #[serde(default)]
    pub payload: Option<Value>,
    /// Re-enqueue interval for recurring jobs; `None` runs once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every: Option<Duration>,
}

type JobFuture = BoxFuture<'static, Result<()>>;

/// Runs one job. Errors are logged and the worker continues.
pub type JobHandler = Arc<dyn Fn(Job) -> JobFuture + Send + Sync>;

#[derive(Clone)]
struct WorkerState {
    handler: JobHandler,
    storage: Storage,
}

/// Redis-backed job queue backed by `apalis-redis`.
#[derive(Clone, Debug)]
pub struct Scheduler {
    storage: Storage,
}

impl Scheduler {
    /// Connect to Redis and build a scheduler.
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let conn = apalis_redis::connect(redis_url)
            .await
            .map_err(|err| LumiError::Cache(err.to_string()))?;
        Ok(Self {
            storage: RedisStorage::new(conn),
        })
    }

    /// Enqueue a job to run as soon as a worker is free.
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

async fn dispatch(job: Job, state: Data<WorkerState>) -> std::result::Result<(), BoxDynError> {
    let Job { name, payload, every } = job;
    if let Err(err) = (state.handler)(Job {
        name: name.clone(),
        payload: payload.clone(),
        every,
    })
    .await
    {
        tracing::warn!(job = %name, error = %err, "scheduled job failed");
    }
    if let Some(every) = every {
        let next = Job {
            name,
            payload,
            every: Some(every),
        };
        let mut storage = state.storage.clone();
        storage
            .push_task(TaskBuilder::new(next).run_after(every).build())
            .await?;
    }
    Ok(())
}
