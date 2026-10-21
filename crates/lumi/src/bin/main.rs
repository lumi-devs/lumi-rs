#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use clap::Parser;
use dashmap::DashMap;
use poise::serenity_prelude as serenity;
use tokio::sync::watch;

use lumi_kernel::bot::Data;
use lumi_kernel::config::ShardRange;
use lumi_kernel::{Config, Job, Role, Scheduler};

#[derive(Parser, Debug)]
#[command(
    name = "lumi",
    version,
    about = "Lumi Discord Bot & Distributed Microservices"
)]
struct Cli {
    /// Process role: all, gateway, api or worker.
    #[arg(default_value = "all")]
    role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();
    init_tracing();

    let cli = Cli::parse();
    let role = lumi::runtime::role_from_arg(&cli.role)?;

    let mut cfg = Config::load()?;
    cfg.role = role;
    cfg.validate_for(role)?;

    tracing::info!(role = ?role, "lumi initialized");

    // One Services instance per process, shared by every role it runs.
    let services = lumi::runtime::services(cfg).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let sig_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        lumi::runtime::shutdown_signal().await;
        sig_tx.send_replace(true);
    });

    match role {
        Role::Api => start_api(services, shutdown_rx).await,
        Role::Worker => start_worker(services, shutdown_rx).await,
        Role::Gateway => start_gateway(services, shutdown_rx, shutdown_tx).await,
        Role::All => {
            tracing::info!("starting all-in-one process (gateway + api + worker)");

            let api = Arc::clone(&services);
            let api_rx = shutdown_rx.clone();
            let api_handle = tokio::spawn(async move {
                if let Err(err) = start_api(api, api_rx).await {
                    tracing::error!(error = %err, "HTTP API service crashed");
                }
            });

            let worker = Arc::clone(&services);
            let worker_rx = shutdown_rx.clone();
            let worker_handle = tokio::spawn(async move {
                if let Err(err) = start_worker(worker, worker_rx).await {
                    tracing::error!(error = %err, "background worker crashed");
                }
            });

            let gateway_res = start_gateway(services, shutdown_rx, shutdown_tx.clone()).await;

            // Signal other tiers that gateway has ended (e.g. lease loss or error)
            shutdown_tx.send_replace(true);

            let bg_drain = async {
                let _ = tokio::join!(api_handle, worker_handle);
            };
            if tokio::time::timeout(lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT, bg_drain)
                .await
                .is_err()
            {
                tracing::warn!(
                    timeout_secs = lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                    "background tasks drain deadline exceeded in all-in-one process"
                );
            }

            gateway_res
        }
    }
}

async fn start_api(services: Data, mut shutdown_rx: watch::Receiver<bool>) -> Result<()> {
    let addr = format!("0.0.0.0:{}", services.cfg.rpc_port).parse()?;
    tracing::info!(addr = %addr, "starting HTTP/RPC API server");

    let (shutdown_tx, shutdown_signal) = tokio::sync::oneshot::channel::<()>();
    let server_fut = lumi_api::serve_with_shutdown(addr, services, async move {
        let _ = shutdown_signal.await;
    });
    tokio::pin!(server_fut);

    tokio::select! {
        res = &mut server_fut => {
            res?;
        }
        _ = shutdown_rx.wait_for(|s| *s) => {
            tracing::info!("HTTP API server received shutdown signal, draining connections");
            let _ = shutdown_tx.send(());

            match tokio::time::timeout(lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT, &mut server_fut).await {
                Ok(res) => res?,
                Err(_) => {
                    tracing::warn!(
                        timeout_secs = lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
                        "HTTP API server drain deadline exceeded; terminating connections"
                    );
                }
            }
        }
    }
    tracing::info!("HTTP API server stopped");
    Ok(())
}

async fn start_worker(services: Data, mut shutdown_rx: watch::Receiver<bool>) -> Result<()> {
    let scheduler = Scheduler::connect(&services.cfg.redis_url).await?;
    tracing::info!("background worker started (no gateway session)");

    let tracker = lumi::runtime::JobDrainTracker::new();

    let handler: lumi_kernel::scheduler::JobHandler = {
        let worker_services = Arc::clone(&services);
        let tracker = tracker.clone();
        Arc::new(move |job| {
            let services = Arc::clone(&worker_services);
            let tracker = tracker.clone();
            Box::pin(async move {
                if tracker.is_closed() {
                    tracing::debug!(job = %job.name, "worker draining; skipping new job dispatch");
                    return Ok(());
                }
                tracker.track(lumi::runtime::run_job(&services, job)).await
            })
        })
    };
    let queue = scheduler.run(handler);
    tokio::pin!(queue);

    let mut interval = tokio::time::interval(lumi::runtime::WORKER_TICK);

    loop {
        tokio::select! {
            () = &mut queue => {
                return Err(anyhow!("background job queue stopped unexpectedly"));
            }
            _ = interval.tick() => {
                let autoupdate = Job {
                    name: lumi::runtime::ADDONS_AUTOUPDATE_JOB.to_string(),
                    payload: None,
                    every: None,
                };
                let tracker_clone = tracker.clone();
                let services_clone = Arc::clone(&services);
                tokio::spawn(async move {
                    if let Err(err) = tracker_clone
                        .track(lumi::runtime::run_job(&services_clone, autoupdate))
                        .await
                    {
                        tracing::warn!(error = %err, "periodic addon auto-update failed");
                    }
                });

                let cleanup = Job {
                    name: lumi::runtime::MAINTENANCE_CLEANUP_JOB.to_string(),
                    payload: None,
                    every: None,
                };
                let tracker_clone = tracker.clone();
                let services_clone = Arc::clone(&services);
                tokio::spawn(async move {
                    if let Err(err) = tracker_clone
                        .track(lumi::runtime::run_job(&services_clone, cleanup))
                        .await
                    {
                        tracing::warn!(error = %err, "periodic maintenance cleanup failed");
                    }
                });
            }
            _ = shutdown_rx.wait_for(|lost| *lost) => {
                tracing::info!("worker received shutdown signal, draining in-flight jobs");
                break;
            }
        }
    }

    tracker.close();
    let drain_fut = async {
        let _ = tokio::select! {
            _ = &mut queue => {},
            () = tracker.wait_idle() => {},
        };
    };
    match tokio::time::timeout(lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT, drain_fut).await {
        Ok(()) => tracing::info!("all background jobs drained successfully"),
        Err(_) => tracing::warn!(
            remaining = tracker.in_flight_count(),
            timeout_secs = lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
            "in-flight job drain deadline exceeded; terminating worker"
        ),
    }

    tracing::info!("worker stopped");
    Ok(())
}

async fn start_gateway(
    services: Data,
    mut shutdown_rx: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let mut leased: Option<ShardRange> = None;
    let mut lease_lost: Option<watch::Receiver<bool>> = None;

    if services.cfg.shard_coordinator && services.cfg.shard_range.is_none() {
        let total = services
            .cfg
            .shard_count
            .ok_or_else(|| anyhow!("SHARD_COORDINATOR requires SHARDS (the fleet total)"))?;
        let node_id =
            std::env::var("HOSTNAME").unwrap_or_else(|_| format!("node-{}", std::process::id()));

        match lumi_kernel::coordinator::lease_shard_range(
            &services.cache,
            total,
            services.cfg.max_shards_per_node,
            &node_id,
        )
        .await?
        {
            Some(range) => {
                tracing::info!(
                    node = %node_id,
                    start = range.start,
                    end = range.end,
                    total,
                    "leased shard range from coordinator"
                );
                lease_lost = Some(lumi_kernel::coordinator::spawn_lease_heartbeat(
                    Arc::clone(&services.cache),
                    range,
                    node_id,
                ));
                leased = Some(range);
            }
            None => {
                return Err(anyhow!(
                    "no unleased shard slots available in fleet of {total} shards"
                ));
            }
        }
    }

    let shard_range = services.cfg.shard_range.or(leased);
    let mut client = lumi::client::build(Arc::clone(&services)).await?;

    let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
    let runners = Arc::clone(&client.shard_manager.runners);
    let mut lease_fenced = false;

    let run = start_shards(&mut client, shard_range, services.cfg.shard_count);
    tokio::pin!(run);

    tokio::select! {
        result = &mut run => return result,
        _ = shutdown_rx.wait_for(|lost| *lost) => {
            tracing::info!("shutdown signal received, canceling shard manager");
        }
        () = wait_for_lease_loss(&mut lease_lost) => {
            lease_fenced = true;
            tracing::error!("shard lease lost, canceling shard manager");
            shutdown_tx.send_replace(true);
        }
    }

    shutdown_trigger();

    for entry in runners.iter() {
        let (shard_id, runner) = entry.pair();
        if let Err(err) = runner
            .tx
            .unbounded_send(serenity::ShardRunnerMessage::Shutdown(1000))
        {
            tracing::warn!(%shard_id, error = %err, "failed to send shutdown to shard");
        }
    }

    tracing::info!("awaiting clean disconnection of gateway shards");
    let disconnect_drain = async {
        let _ = (&mut run).await;
        await_runners_disconnected(&runners).await;
    };

    match tokio::time::timeout(lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT, disconnect_drain).await {
        Ok(()) => tracing::info!("gateway shards cleanly disconnected"),
        Err(_) => tracing::warn!(
            timeout_secs = lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
            "gateway shard disconnect drain deadline exceeded"
        ),
    }

    if lease_fenced {
        return Err(anyhow!(
            "shard lease lost; exiting so the orchestrator can re-lease the shards"
        ));
    }
    Ok(())
}

async fn await_runners_disconnected(
    runners: &DashMap<serenity::ShardId, serenity::ShardRunnerMetadata>,
) {
    loop {
        if runners.iter().all(|r| r.tx.is_closed()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_for_lease_loss(lease: &mut Option<watch::Receiver<bool>>) {
    match lease {
        Some(receiver) => {
            // A closed channel means the heartbeat died; treat it as a loss.
            if receiver.wait_for(|lost| *lost).await.is_err() {
                tracing::error!("shard lease heartbeat stopped unexpectedly");
            }
        }
        None => std::future::pending().await,
    }
}

async fn start_shards(
    client: &mut serenity::Client,
    shard_range: Option<ShardRange>,
    total: Option<u16>,
) -> Result<()> {
    match (shard_range, total) {
        (Some(range), Some(total)) => {
            // Serenity's range end is INCLUSIVE: start_shard_range(4..7, 10) runs
            // shards 4 through 7, while start_shards(n) means 0..n-1.
            tracing::info!(
                start = range.start,
                end = range.end,
                total,
                "starting shard range"
            );
            client
                .start_shard_range(range.start..range.end, total)
                .await?;
        }
        (None, Some(total)) => {
            tracing::info!(total, "starting forced shard count");
            client.start_shards(total).await?;
        }
        (None, None) => client.start_autosharded().await?,
        (Some(range), None) => {
            return Err(anyhow!(
                "SHARD_RANGE {}-{} is set but SHARDS (fleet total) is missing",
                range.start,
                range.end
            ));
        }
    }
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,lumi=debug"));
    fmt().with_env_filter(filter).init();
}
