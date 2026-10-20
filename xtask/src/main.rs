//! Small operational helpers for the workspace. Kept out of the bot binary so
//! CI and container entrypoints can migrate a database without a Discord token.

use anyhow::{Context, Result, bail};
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_target(false).init();
    tokio::runtime::Runtime::new()
        .context("failed to start the tokio runtime")?
        .block_on(run())
}

async fn run() -> Result<()> {
    let command = std::env::args().nth(1);
    match command.as_deref() {
        Some("migrate") => migrate().await,
        Some(command) => bail!("unknown command `{command}`: expected `migrate`"),
        None => {
            info!("usage: xtask migrate");
            Ok(())
        }
    }
}

async fn migrate() -> Result<()> {
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let db = lumi_db::connect(&url)
        .await
        .context("failed to connect to the database")?;
    lumi_db::migrate(&db).await.context("migration failed")?;
    info!("database schema is up to date");
    Ok(())
}
