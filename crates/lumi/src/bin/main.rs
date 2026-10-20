#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "lumi",
    version,
    about = "Lumi Discord Bot & Distributed Microservices"
)]
struct Cli {
    #[arg(default_value = "all")]
    role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();
    init_tracing();

    let cli = Cli::parse();
    let mut cfg = lumi_kernel::Config::load()?;

    match cli.role.as_str() {
        "all" => cfg.role = lumi_kernel::Role::All,
        "gateway" | "shard" => cfg.role = lumi_kernel::Role::Gateway,
        "worker" => cfg.role = lumi_kernel::Role::Worker,
        "api" => cfg.role = lumi_kernel::Role::Api,
        "scheduler" => cfg.role = lumi_kernel::Role::Scheduler,
        other => {
            tracing::warn!(role = %other, "unrecognized CLI role argument, falling back to config role");
        }
    }

    tracing::info!(role = ?cfg.role, "lumi initialized");

    match cfg.role {
        lumi_kernel::Role::Api => {
            start_api(cfg).await?;
        }
        lumi_kernel::Role::Worker => {
            start_worker(cfg).await?;
        }
        lumi_kernel::Role::Scheduler => {
            start_scheduler(cfg).await?;
        }
        lumi_kernel::Role::Gateway => {
            start_gateway(cfg).await?;
        }
        lumi_kernel::Role::All => {
            tracing::info!("starting All-In-One supervisor (Gateway + API + Scheduler + Worker)");
            let api_cfg = cfg.clone();

            // 1. Spawn HTTP API in background
            tokio::spawn(async move {
                if let Err(e) = start_api(api_cfg).await {
                    tracing::error!(error = %e, "HTTP API service crashed");
                }
            });

            // 2. Run Gateway as foreground supervisor
            start_gateway(cfg).await?;
        }
    }

    Ok(())
}

async fn start_api(cfg: lumi_kernel::Config) -> Result<()> {
    let addr = format!("0.0.0.0:{}", cfg.rpc_port).parse()?;
    let token = cfg.rpc_token.clone();
    let services = lumi::runtime::services(cfg.clone()).await?;
    tracing::info!(addr = %addr, "starting HTTP/RPC API server");
    lumi_api::serve(addr, token, &cfg, services).await?;
    Ok(())
}

async fn start_gateway(cfg: lumi_kernel::Config) -> Result<()> {
    let dynamic_range = if cfg.shard_coordinator && cfg.shard_range.is_none() {
        if let Some(total) = cfg.shard_count {
            let cache = lumi_kernel::Cache::connect(&cfg.redis_url).await?;
            let node_id = std::env::var("HOSTNAME")
                .unwrap_or_else(|_| format!("node-{}", std::process::id()));
            let leased = lumi_kernel::coordinator::lease_shard_range(
                &cache,
                total,
                cfg.max_shards_per_node,
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
                lumi_kernel::coordinator::spawn_lease_heartbeat(
                    std::sync::Arc::new(cache),
                    range,
                    node_id,
                );
                Some(range)
            } else {
                return Err(anyhow::anyhow!(
                    "no unleased shard slots available in fleet of {total} shards"
                ));
            }
        } else {
            return Err(anyhow::anyhow!(
                "SHARD_COORDINATOR requires SHARDS (fleet total) to be configured"
            ));
        }
    } else {
        None
    };

    let shard_range = cfg.shard_range.or(dynamic_range);
    let shards = (shard_range, cfg.shard_count);
    let mut client = lumi::client::build(cfg).await?;
    let shutdown_trigger = client.shard_manager.get_shutdown_trigger();

    tokio::spawn(async move {
        lumi::runtime::shutdown_signal().await;
        tracing::info!("shutdown signal received, closing gateway shards");
        shutdown_trigger();
    });

    match shards {
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
            return Err(anyhow::anyhow!(
                "SHARD_RANGE {}-{} is set but SHARDS (fleet total) is missing",
                range.start,
                range.end
            ));
        }
    }
    Ok(())
}

async fn start_worker(cfg: lumi_kernel::Config) -> Result<()> {
    tracing::info!("starting Lumi background worker tier");
    let _cache = lumi_kernel::Cache::connect(&cfg.redis_url).await?;
    let _db = lumi_db::connect(&cfg.database_url).await?;

    // Worker background loop listening on Redis stream/pubsub
    let shutdown = lumi::runtime::shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("worker received shutdown signal, draining jobs");
                break;
            }
            () = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                // Background worker iteration (e.g. invalidate expired caches, process queues)
            }
        }
    }
    Ok(())
}

async fn start_scheduler(cfg: lumi_kernel::Config) -> Result<()> {
    tracing::info!("starting Lumi scheduler cron runner");
    let _db = lumi_db::connect(&cfg.database_url).await?;

    let shutdown = lumi::runtime::shutdown_signal();
    tokio::pin!(shutdown);

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    loop {
        tokio::select! {
            () = &mut shutdown => {
                tracing::info!("scheduler received shutdown signal");
                break;
            }
            _ = interval.tick() => {
                tracing::debug!("executing scheduled maintenance ticks");
            }
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
