#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "lumi", version, about = "Lumi Discord Bot & Services")]
struct Cli {
    #[arg(default_value = "all")]
    role: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
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

    tracing::info!(role = ?cfg.role, "lumi starting");

    match cfg.role {
        lumi_kernel::Role::Api => {
            let addr = format!("0.0.0.0:{}", cfg.rpc_port).parse()?;
            let db = lumi_db::connect(&cfg.database_url).await?;
            let cache = std::sync::Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
            let permits = std::sync::Arc::new(lumi_kernel::PermitResolver::load(&db).await?);
            let registry = lumi_kernel::ModuleRegistry::new();
            let filter = std::sync::Arc::new(lumi_kernel::ContentFilter::default());
            let services = std::sync::Arc::new(lumi_kernel::Services {
                cfg: cfg.clone(),
                db,
                cache,
                permits,
                registry,
                filter,
            });
            lumi_api::serve(addr, cfg.rpc_token, services).await?;
        }
        _ => {
            let mut client = lumi::client::build(cfg).await?;
            let shutdown_trigger = client.shard_manager.get_shutdown_trigger();

            tokio::spawn(async move {
                shutdown_signal().await;
                tracing::info!("shutdown signal received");
                shutdown_trigger();
            });

            client.start().await?;
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
