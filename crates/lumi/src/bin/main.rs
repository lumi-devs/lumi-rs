#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use clap::Parser;
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

    match role {
        Role::Api => start_api(services).await,
        Role::Worker => start_worker(services).await,
        Role::Gateway => start_gateway(services).await,
        Role::All => {
            tracing::info!("starting all-in-one process (gateway + api + worker)");

            let api = Arc::clone(&services);
            tokio::spawn(async move {
                if let Err(err) = start_api(api).await {
                    tracing::error!(error = %err, "HTTP API service crashed");
                }
            });

            let worker = Arc::clone(&services);
            tokio::spawn(async move {
                if let Err(err) = start_worker(worker).await {
                    tracing::error!(error = %err, "background worker crashed");
                }
            });

            start_gateway(services).await
        }
    }
}

async fn start_api(services: Data) -> Result<()> {
    let addr = format!("0.0.0.0:{}", services.cfg.rpc_port).parse()?;
    tracing::info!(addr = %addr, "starting HTTP/RPC API server");
    lumi_api::serve(addr, services).await?;
    Ok(())
}

async fn start_worker(services: Data) -> Result<()> {
    let scheduler = Scheduler::connect(&services.cfg.redis_url).await?;
    tracing::info!("background worker started (no gateway session)");

    let handler: lumi_kernel::scheduler::JobHandler = {
        let worker_services = Arc::clone(&services);
        Arc::new(move |job| {
            let services = Arc::clone(&worker_services);
            Box::pin(async move { lumi::runtime::run_job(&services, job).await })
        })
    };
    let queue = scheduler.run(handler);
    tokio::pin!(queue);

    tokio::select! {
        () = &mut queue => {
            return Err(anyhow!("background job queue stopped unexpectedly"));
        }
        () = worker_tick(&services) => {}
        () = lumi::runtime::shutdown_signal() => {
            tracing::info!("worker received shutdown signal");
        }
    }
    Ok(())
}

/// Periodic maintenance that must run once per fleet, not once per gateway pod.
async fn worker_tick(services: &Data) {
    let mut interval = tokio::time::interval(lumi::runtime::WORKER_TICK);
    loop {
        interval.tick().await;
        let job = Job {
            name: lumi::runtime::ADDONS_AUTOUPDATE_JOB.to_string(),
            payload: None,
            every: None,
        };
        if let Err(err) = lumi::runtime::run_job(services, job).await {
            tracing::warn!(error = %err, "periodic worker job failed");
        }
    }
}

async fn start_gateway(services: Data) -> Result<()> {
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
    let mut lease_fenced = false;

    {
        let run = start_shards(&mut client, shard_range, services.cfg.shard_count);
        tokio::pin!(run);

        tokio::select! {
            result = &mut run => result?,
            () = lumi::runtime::shutdown_signal() => {
                tracing::info!("shutdown signal received, closing gateway shards");
            }
            () = wait_for_lease_loss(&mut lease_lost) => {
                lease_fenced = true;
                tracing::error!("shard lease lost, closing gateway shards");
            }
        }
    }

    shutdown_trigger();

    if lease_fenced {
        return Err(anyhow!(
            "shard lease lost; exiting so the orchestrator can re-lease the shards"
        ));
    }
    Ok(())
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
