#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use clap::Parser;
use poise::serenity_prelude as serenity;
use tokio::sync::watch;

use lumi_kernel::bot::Data;
use lumi_kernel::config::ShardRange;
use lumi_kernel::{Config, Role, Scheduler};

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
    lumi_api::init_metrics();

    let cli = Cli::parse();
    let role = lumi::runtime::role_from_arg(&cli.role)?;

    let mut cfg = Config::load()?;
    cfg.role = role;
    cfg.validate_for(role)?;

    tracing::info!(role = ?role, "lumi initialized");

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
        _ = async {
            let _ = shutdown_rx.wait_for(|s| *s).await;
        } => {
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

    let worker_services = Arc::clone(&services);
    let handler: lumi_kernel::scheduler::JobHandler = Arc::new(move |job| {
        let services = Arc::clone(&worker_services);
        Box::pin(async move { lumi::runtime::run_job(&services, job).await })
    });

    let queue = scheduler.run(handler);
    tokio::pin!(queue);

    let mut interval = tokio::time::interval(lumi::runtime::WORKER_TICK);
    loop {
        tokio::select! {
            () = &mut queue => {
                return Err(anyhow!("background job queue stopped unexpectedly"));
            }
            _ = interval.tick() => {
                let autoupdate = lumi_kernel::Job {
                    name: lumi::runtime::ADDONS_AUTOUPDATE_JOB.to_string(),
                    payload: None,
                    every: None,
                    attempt: 0,
                };
                let services_clone = Arc::clone(&services);
                tokio::spawn(async move {
                    if let Err(err) = lumi::runtime::run_job(&services_clone, autoupdate).await {
                        tracing::warn!(error = %err, "periodic addon auto-update failed");
                    }
                });
            }
            _ = async {
                let _ = shutdown_rx.wait_for(|lost| *lost).await;
            } => {
                tracing::info!("worker received shutdown signal");
                break;
            }
        }
    }
    tracing::info!("background worker stopped");
    Ok(())
}

async fn start_gateway(
    services: Data,
    mut shutdown_rx: watch::Receiver<bool>,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let mut lease: Option<(
        Arc<lumi_kernel::Cache>,
        ShardRange,
        String,
        lumi_kernel::coordinator::LeaseHeartbeat,
    )> = None;

    let dynamic_range = if services.cfg.shard_coordinator && services.cfg.shard_range.is_none() {
        let total = services
            .cfg
            .shard_count
            .ok_or_else(|| anyhow!("SHARD_COORDINATOR requires SHARDS (the fleet total)"))?;
        let node_id =
            std::env::var("HOSTNAME").unwrap_or_else(|_| format!("node-{}", std::process::id()));
        let leased = lumi_kernel::coordinator::lease_shard_range(
            &services.cache,
            total,
            services.cfg.max_shards_per_node,
            &node_id,
        )
        .await?;
        if let Some(range) = leased {
            tracing::info!(
                node = %node_id,
                start = range.start,
                end = range.end,
                total,
                "leased shard range from coordinator"
            );
            let heartbeat = lumi_kernel::coordinator::spawn_lease_heartbeat(
                Arc::clone(&services.cache),
                range,
                node_id.clone(),
            );
            lease = Some((Arc::clone(&services.cache), range, node_id, heartbeat));
            Some(range)
        } else {
            return Err(anyhow!(
                "no unleased shard slots available in fleet of {total} shards"
            ));
        }
    } else {
        None
    };

    let shard_range = services.cfg.shard_range.or(dynamic_range);
    let mut client = lumi::client::build(Arc::clone(&services)).await?;
    let shutdown_trigger = client.shard_manager.get_shutdown_trigger();
    let runners = Arc::clone(&client.shard_manager.runners);

    let mut lease_lost_rx = lease.as_ref().map(|l| l.3.lost());
    let mut lease_fenced = false;

    let run = start_shards(&mut client, shard_range, services.cfg.shard_count);
    tokio::pin!(run);

    tokio::select! {
        result = &mut run => {
            if let Some((cache, range, node_id, heartbeat)) = &lease {
                heartbeat.abort();
                let _ = lumi_kernel::coordinator::release_shard_range(cache, *range, node_id).await;
            }
            return result;
        }
        _ = async {
            let _ = shutdown_rx.wait_for(|lost| *lost).await;
        } => {
            tracing::info!("shutdown signal received, canceling shard manager");
        }
        () = async {
            match &mut lease_lost_rx {
                Some(rx) => {
                    while !*rx.borrow() {
                        if rx.changed().await.is_err() {
                            return;
                        }
                    }
                }
                None => std::future::pending::<()>().await,
            }
        } => {
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
        while !runners.iter().all(|r| r.tx.is_closed()) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    match tokio::time::timeout(lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT, disconnect_drain).await {
        Ok(()) => tracing::info!("gateway shards cleanly disconnected"),
        Err(_) => tracing::warn!(
            timeout_secs = lumi::runtime::SHUTDOWN_DRAIN_TIMEOUT.as_secs(),
            "gateway shard disconnect drain deadline exceeded"
        ),
    }

    if let Some((cache, range, node_id, heartbeat)) = &lease {
        heartbeat.abort();
        if !lease_fenced {
            if let Err(error) =
                lumi_kernel::coordinator::release_shard_range(cache, *range, node_id).await
            {
                tracing::warn!(%error, "failed to release shard leases");
            } else {
                tracing::info!(start = range.start, end = range.end, "released shard leases");
            }
        }
    }

    if lease_fenced {
        return Err(anyhow!(
            "shard lease lost; exiting so orchestrator can reassign shards"
        ));
    }
    Ok(())
}

async fn start_shards(
    client: &mut serenity::Client,
    range: Option<ShardRange>,
    total: Option<u16>,
) -> Result<()> {
    match (range, total) {
        (Some(range), Some(total)) => {
            tracing::info!(start = range.start, end = range.end, total, "starting shard range");
            client.start_shard_range(range.start_range(), total).await?;
        }
        (None, Some(total)) => {
            tracing::info!(total, "starting serenity client across configured shard count");
            client.start_shards(total).await?;
        }
        _ => {
            tracing::info!("starting serenity client with Discord recommended shard count");
            client.start_autosharded().await?;
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
