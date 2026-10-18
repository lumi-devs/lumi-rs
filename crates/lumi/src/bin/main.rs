#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "lumi", version, about = "Lumi Discord Bot & Distributed Microservices")]
struct Cli {
    #[arg(default_value = "all")]
    role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");
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
    let db = lumi_db::connect(&cfg.database_url).await?;
    let cache = Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi_kernel::PermitResolver::load(&db).await?);
    let registry = lumi_kernel::ModuleRegistry::new();
    let security = Arc::new(lumi_kernel::SecurityManager::new(db.clone()));
    let filter = Arc::new(lumi_kernel::ContentFilter::default());
    let downloader = Arc::new(lumi_kernel::Downloader::new(
        db.clone(),
        std::path::PathBuf::from(&cfg.custom_dir),
    ));
    let services = Arc::new(lumi_kernel::Services {
        cfg: cfg.clone(),
        db,
        cache,
        permits,
        registry,
        security,
        filter,
        downloader,
    });
    tracing::info!(addr = %addr, "starting HTTP/RPC API server");
    lumi_api::serve(addr, cfg.rpc_token, services).await?;
    Ok(())
}

async fn start_gateway(cfg: lumi_kernel::Config) -> Result<()> {
    let mut client = lumi::client::build(cfg).await?;
    let shutdown_trigger = client.shard_manager.get_shutdown_trigger();

    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received, closing gateway shards");
        shutdown_trigger();
    });

    client.start().await?;
    Ok(())
}

async fn start_worker(cfg: lumi_kernel::Config) -> Result<()> {
    tracing::info!("starting Lumi background worker tier");
    let _cache = lumi_kernel::Cache::connect(&cfg.redis_url).await?;
    let _db = lumi_db::connect(&cfg.database_url).await?;

    // Worker background loop listening on Redis stream/pubsub
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                tracing::info!("worker received shutdown signal, draining jobs");
                break;
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {
                // Background worker iteration (e.g. invalidate expired caches, process queues)
            }
        }
    }
    Ok(())
}

async fn start_scheduler(cfg: lumi_kernel::Config) -> Result<()> {
    tracing::info!("starting Lumi scheduler cron runner");
    let _db = lumi_db::connect(&cfg.database_url).await?;

    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = &mut shutdown => {
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
    use tracing_subscriber::{fmt, EnvFilter};

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,lumi=debug"));
    fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "failed to register ctrl_c handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to register sigterm handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
