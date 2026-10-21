//! Shared process wiring: module registry, service construction and the
//! role entry points.
//!
//! Every process tier (gateway, API, worker) builds the same [`Services`]
//! instance here, so the module set, database handles and cache live in
//! exactly one place.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use poise::serenity_prelude as serenity;

use lumi_kernel::bot::Data;
use lumi_kernel::{Config, Job, ModuleRegistry, Role, Services};

/// Recurring addon auto-update job owned by the worker tier.
pub const ADDONS_AUTOUPDATE_JOB: &str = "addons.autoupdate";
/// How often the worker asks whether a scheduled job is due.
pub const WORKER_TICK: Duration = Duration::from_secs(60);
const AUTO_UPDATE_LOCK: &str = "lumi:job:addons.autoupdate";

/// Map the CLI role argument to a [`Role`].
///
/// # Errors
///
/// Returns an error for an unknown role, including the retired `scheduler`
/// alias.
pub fn role_from_arg(role: &str) -> Result<Role> {
    match role {
        "all" => Ok(Role::All),
        "gateway" | "shard" => Ok(Role::Gateway),
        "worker" => Ok(Role::Worker),
        "api" => Ok(Role::Api),
        "scheduler" => Err(anyhow!(
            "role 'scheduler' was merged into 'worker'; run the worker tier instead"
        )),
        other => Err(anyhow!(
            "unknown role '{other}' (expected all, gateway, worker or api)"
        )),
    }
}

/// Every feature module the host knows about, in display order.
///
/// Registration lives here so the gateway, API, worker and scheduler all see
/// exactly the same module set.
#[must_use]
pub fn registry() -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(lumi_modules::afk::AfkModule));
    registry.register(Box::new(lumi_modules::logging::LoggingModule));
    registry.register(Box::new(lumi_modules::filter::FilterModule));
    registry.register(Box::new(lumi_modules::moderation::ModerationModule));
    registry.register(Box::new(lumi_modules::security::SecurityModule));
    registry.register(Box::new(lumi_modules::tempvc::TempVcModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));
    registry.register(Box::new(lumi_modules::economy::EconomyModule));
    registry.register(Box::new(lumi_modules::leveling::LevelingModule));
    registry
}

/// Connect every shared backend, run pending migrations and build the
/// process-wide [`Services`].
///
/// Only gateway roles resolve application owners, because that costs a Discord
/// REST round-trip the API and worker tiers do not need.
///
/// # Errors
///
/// Returns an error when any backend cannot be reached or a migration fails.
pub async fn services(mut cfg: Config) -> Result<Data> {
    if cfg.role.is_gateway() {
        resolve_owners(&mut cfg).await;
    }
    Services::bootstrap(cfg, registry()).await
}

async fn resolve_owners(cfg: &mut Config) {
    let Ok(token) = cfg.token.parse::<serenity::Token>() else {
        tracing::warn!("gateway token could not be parsed; skipping owner resolution");
        return;
    };
    let http = serenity::Http::new(token);

    match http.get_current_application_info().await {
        Ok(info) => {
            if let Some(owner) = info.owner {
                push_owner(cfg, owner.id.get(), "application owner");
            }
            if let Some(team) = info.team {
                push_owner(cfg, team.owner_user_id.get(), "team owner");
                for member in team.members {
                    if member.membership_state == serenity::MembershipState::Accepted {
                        push_owner(cfg, member.user.id.get(), "team member");
                    }
                }
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "failed to resolve application owner from Discord HTTP");
        }
    }
}

fn push_owner(cfg: &mut Config, id: u64, source: &str) {
    if !cfg.owner_ids.contains(&id) {
        tracing::info!(owner_id = id, source, "resolved application owner");
        cfg.owner_ids.push(id);
    }
}

/// Dispatch one background job. Unknown names fail loudly so a typo in an
/// enqueue call is visible in logs instead of being dropped.
///
/// # Errors
///
/// Returns an error when the job name is unknown or the job itself fails.
pub async fn run_job(services: &Services, job: Job) -> lumi_kernel::Result<()> {
    match job.name.as_str() {
        ADDONS_AUTOUPDATE_JOB => auto_update_addons(services).await,
        other => Err(lumi_kernel::LumiError::Invalid(format!(
            "unknown background job: {other}"
        ))),
    }
}

async fn auto_update_addons(services: &Services) -> lumi_kernel::Result<()> {
    let cfg = services.downloader.get_auto_update_config().await?;
    if !cfg.enabled {
        return Ok(());
    }

    // The lock makes the job safe when several workers start at once.
    let interval_ms = cfg.interval_minutes.max(1).saturating_mul(60_000);
    if !services
        .cache
        .claim_cooldown(AUTO_UPDATE_LOCK, interval_ms)
        .await?
    {
        return Ok(());
    }

    for repo in services.downloader.check_for_updates_detailed().await? {
        match services.downloader.update_repo(&repo).await {
            Ok(result) if result.changed => {
                tracing::info!(repo = %repo, "addon repository auto-updated");
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(repo = %repo, error = %err, "addon auto-update failed");
            }
        }
    }
    Ok(())
}

/// Wrap user data for [`serenity::ClientBuilder::data`].
///
/// Poise's user-data type `U` is [`Data`] (`Arc<Services>`), and `builder.data`
/// takes `Arc<U>`, so the client must be handed `Arc<Arc<Services>>`. Routing
/// every call through this typed helper keeps `Context::data::<Data>()` and the
/// `Arc<Arc<Services>>` downcast from silently drifting apart.
#[must_use]
pub fn client_data(data: Data) -> Arc<Data> {
    Arc::new(data)
}

/// Resolve once Ctrl-C or SIGTERM arrives.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "failed to register ctrl_c handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_data_keeps_poise_user_data_shape() {
        // Compile-time proof: `client_data` only accepts `Data` and yields
        // `Arc<Data>`, the exact type `ClientBuilder::data` and the
        // `Context::data::<Data>()` downcast require.
        let _: fn(Data) -> Arc<Data> = client_data;
    }

    #[test]
    fn registry_is_non_empty_and_unique() {
        let registry = registry();
        let names: Vec<_> = registry.iter().map(lumi_kernel::Module::name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "duplicate module registered");
        assert!(!registry.is_empty());
    }

    #[test]
    fn roles_parse_and_aliases_hold() {
        assert_eq!(role_from_arg("all").ok(), Some(Role::All));
        assert_eq!(role_from_arg("gateway").ok(), Some(Role::Gateway));
        assert_eq!(role_from_arg("shard").ok(), Some(Role::Gateway));
        assert_eq!(role_from_arg("worker").ok(), Some(Role::Worker));
        assert_eq!(role_from_arg("api").ok(), Some(Role::Api));
    }

    #[test]
    fn unknown_and_merged_roles_fail_fast() {
        for role in ["scheduler", "shards", ""] {
            assert!(role_from_arg(role).is_err(), "role '{role}' was accepted");
        }
    }
}
