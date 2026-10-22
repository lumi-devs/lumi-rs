use poise::serenity_prelude as serenity;
use serenity::all::{
    EditMember, EditRole, GenericChannelId, Guild, GuildId, Member, PartialGuild, Permissions,
    Role, RoleId, Timestamp, UserId,
};
use serenity::model::guild::audit_log as audit;

use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_kernel::security::{ActionThresholdViolation, AntiNukeResponse, DestructiveActionKind};
use lumi_ui::{Card, CardKind};

use super::store;

/// Audit entries older than this are ignored (mirrors lumi's 10s window).
const AUDIT_GRACE_SECS: i64 = 10;

/// Permissions whose grant to `@everyone` is reverted and counted.
const DANGEROUS_PERMISSIONS: [Permissions; 6] = [
    Permissions::ADMINISTRATOR,
    Permissions::MANAGE_GUILD,
    Permissions::MANAGE_ROLES,
    Permissions::MANAGE_CHANNELS,
    Permissions::BAN_MEMBERS,
    Permissions::KICK_MEMBERS,
];

/// Map a Discord audit-log action to the destructive kind we score.
const fn destructive_kind(action: audit::Action) -> Option<DestructiveActionKind> {
    match action {
        audit::Action::Member(audit::MemberAction::BanAdd) => Some(DestructiveActionKind::Ban),
        audit::Action::Member(audit::MemberAction::Kick | audit::MemberAction::Prune) => {
            Some(DestructiveActionKind::Kick)
        }
        audit::Action::Channel(audit::ChannelAction::Delete) => {
            Some(DestructiveActionKind::ChannelDelete)
        }
        audit::Action::Role(audit::RoleAction::Delete) => Some(DestructiveActionKind::RoleDelete),
        audit::Action::Webhook(audit::WebhookAction::Create) => {
            Some(DestructiveActionKind::WebhookCreate)
        }
        _ => None,
    }
}

/// Audited destructive action. Entry `GuildAuditLogEntryCreate`.
pub async fn on_audit_log_entry(
    ctx: &serenity::Context,
    services: &Services,
    entry: &audit::AuditLogEntry,
    guild_id: GuildId,
) -> Result<(), Error> {
    let Some(kind) = destructive_kind(entry.action) else {
        return Ok(());
    };
    let Some(executor) = entry.user_id else {
        return Ok(());
    };
    evaluate(ctx, services, guild_id, executor, kind).await
}

/// Revert a dangerous `@everyone` permission grant, then score it.
pub async fn on_role_update(
    ctx: &serenity::Context,
    services: &Services,
    old: Option<&Role>,
    new: &Role,
) -> Result<(), Error> {
    if new.id != new.guild_id.everyone_role() {
        return Ok(());
    }
    let Some(old) = old else {
        return Ok(());
    };

    let granted = DANGEROUS_PERMISSIONS
        .iter()
        .any(|perm| new.permissions.contains(*perm) && !old.permissions.contains(*perm));
    if !granted {
        return Ok(());
    }

    if let Err(error) = new
        .guild_id
        .edit_role(
            &ctx.http,
            new.id,
            EditRole::new()
                .permissions(old.permissions)
                .audit_log_reason("Security: reverted dangerous permission grant on @everyone"),
        )
        .await
    {
        tracing::warn!(%error, "failed to revert @everyone permission grant");
    }

    let Some(executor) = audit_executor(
        ctx,
        new.guild_id,
        audit::Action::Role(audit::RoleAction::Update),
        Some(new.id.get()),
        None,
    )
    .await
    else {
        return Ok(());
    };
    evaluate(
        ctx,
        services,
        new.guild_id,
        executor,
        DestructiveActionKind::DangerousPermissionGrant,
    )
    .await
}

/// Revert a quarantined member's roles, then score the bypass attempt.
pub async fn on_member_update(
    ctx: &serenity::Context,
    services: &Services,
    old: Option<&Member>,
    new: Option<&Member>,
) -> Result<(), Error> {
    let Some(new) = new else {
        return Ok(());
    };
    if new.user.bot() {
        return Ok(());
    }
    let guild_id = new.guild_id;
    let Some(role) =
        crate::moderation::store::quarantine_role(&services.cache, &services.db, guild_id.get())
            .await?
    else {
        return Ok(());
    };

    let quarantine = RoleId::new(role);
    if !new.roles.contains(&quarantine) {
        return Ok(());
    }
    if new.roles.len() <= 1 {
        return Ok(());
    }
    if old.is_some_and(|old| old.roles == new.roles) {
        return Ok(());
    }

    if let Err(error) = guild_id
        .edit_member(
            &ctx.http,
            new.user.id,
            EditMember::new().roles(vec![quarantine]),
        )
        .await
    {
        tracing::warn!(%error, "failed to revert quarantine bypass");
    }

    let Some(executor) = audit_executor(
        ctx,
        guild_id,
        audit::Action::Member(audit::MemberAction::RoleUpdate),
        Some(new.user.id.get()),
        None,
    )
    .await
    else {
        return Ok(());
    };
    evaluate(
        ctx,
        services,
        guild_id,
        executor,
        DestructiveActionKind::QuarantineBypass,
    )
    .await
}

/// Load config, exempt trusted actors, then score the action and respond.
async fn evaluate(
    ctx: &serenity::Context,
    services: &Services,
    guild_id: GuildId,
    executor: UserId,
    kind: DestructiveActionKind,
) -> Result<(), Error> {
    let settings = store::anti_nuke_settings(&services.cache, &services.db, guild_id.get()).await?;
    if !settings.config.enabled {
        return Ok(());
    }
    if is_exempt(ctx, guild_id, executor, &settings.trusted_roles) {
        return Ok(());
    }

    let Some(violation) = services.security.record_destructive_action(
        &guild_id.to_string(),
        &executor.to_string(),
        kind,
        &settings.config,
    ) else {
        return Ok(());
    };

    tracing::warn!(
        guild_id = guild_id.get(),
        executor = executor.get(),
        kind = kind.as_str(),
        count = violation.count,
        "anti-nuke threshold tripped"
    );
    respond(
        ctx, services, guild_id, executor, kind, violation, &settings,
    )
    .await
}

/// The guild owner, the bot itself and trusted roles are never acted on.
fn is_exempt(
    ctx: &serenity::Context,
    guild_id: GuildId,
    executor: UserId,
    trusted_roles: &[u64],
) -> bool {
    if executor == ctx.cache.current_user().id {
        return true;
    }
    let Some(guild) = ctx.cache.guild(guild_id) else {
        return false;
    };
    if guild.owner_id == executor {
        return true;
    }
    if trusted_roles.is_empty() {
        return false;
    }
    guild.members.get(&executor).is_some_and(|member| {
        member
            .roles
            .iter()
            .any(|role| trusted_roles.contains(&role.get()))
    })
}

/// Execute the configured response, record an alert case and post the log card.
async fn respond(
    ctx: &serenity::Context,
    services: &Services,
    guild_id: GuildId,
    executor: UserId,
    kind: DestructiveActionKind,
    violation: ActionThresholdViolation,
    settings: &store::AntiNukeSettings,
) -> Result<(), Error> {
    let reason = format!(
        "Anti-nuke: {} {} action(s) in {}s",
        violation.count,
        kind.as_str().replace('_', " "),
        settings.config.window_secs
    );

    match violation.response {
        AntiNukeResponse::Log => {}
        AntiNukeResponse::Quarantine => {
            if let Some(role) = crate::moderation::store::quarantine_role(
                &services.cache,
                &services.db,
                guild_id.get(),
            )
            .await?
                && let Err(error) = guild_id
                    .add_member_role(&ctx.http, executor, RoleId::new(role), Some(&reason))
                    .await
            {
                tracing::warn!(%error, "anti-nuke quarantine failed");
            }
        }
        AntiNukeResponse::Ban => {
            if let Err(error) = guild_id.ban(&ctx.http, executor, 0, Some(&reason)).await {
                tracing::warn!(%error, "anti-nuke ban failed");
            }
        }
    }

    let bot_id = ctx.cache.current_user().id.get();
    if let Err(error) = store::create_alert_case(
        &services.db,
        guild_id.get(),
        executor.get(),
        bot_id,
        &reason,
    )
    .await
    {
        tracing::warn!(%error, "failed to record anti-nuke alert case");
    }

    let Some(channel) = settings.log_channel else {
        tracing::warn!(
            guild_id = guild_id.get(),
            "anti-nuke alert with no log channel configured"
        );
        return Ok(());
    };
    let response_label = match violation.response {
        AntiNukeResponse::Log => "logged",
        AntiNukeResponse::Quarantine => "quarantined",
        AntiNukeResponse::Ban => "banned",
    };
    let card = Card::lines(
        "🚨 Anti-Nuke Alert",
        vec![
            format!("**Executor**: <@{}> (`{}`)", executor.get(), executor.get()),
            format!(
                "**Trigger**: `{}` ×{} in {}s (limit {})",
                kind.as_str(),
                violation.count,
                settings.config.window_secs,
                violation.limit
            ),
            format!("**Response**: {response_label}"),
        ],
    )
    .kind(CardKind::Error)
    .into_reply();
    GenericChannelId::new(channel)
        .send_message(&ctx.http, card.message())
        .await?;
    Ok(())
}

/// Resolve the executor of the most recent matching audit entry, if fresh.
/// `target` filters on the affected entity; `change_key` filters on a specific
/// audit-log change field (e.g. `vanity_url_code`).
async fn audit_executor(
    ctx: &serenity::Context,
    guild_id: GuildId,
    action: audit::Action,
    target: Option<u64>,
    change_key: Option<&str>,
) -> Option<UserId> {
    let logs = guild_id
        .audit_logs(&ctx.http, Some(action), None, None, None, None)
        .await
        .ok()?;
    let entry = logs.entries.iter().find(|entry| {
        target
            .is_none_or(|target| entry.target_id.map(serenity::all::GenericId::get) == Some(target))
            && change_key.is_none_or(|key| entry.changes.iter().any(|change| change.key() == key))
    })?;
    let age = Timestamp::now().unix_timestamp() - entry.id.created_at().unix_timestamp();
    if age > AUDIT_GRACE_SECS {
        return None;
    }
    entry.user_id
}

/// Vanity URL change. Entry `GuildUpdate`.
pub async fn on_guild_update(
    ctx: &serenity::Context,
    services: &Services,
    old: Option<&Guild>,
    new: &PartialGuild,
) -> Result<(), Error> {
    let Some(old) = old else {
        return Ok(());
    };
    if old.vanity_url_code == new.vanity_url_code {
        return Ok(());
    }
    let Some(executor) = audit_executor(
        ctx,
        new.id,
        audit::Action::GuildUpdate,
        None,
        Some("vanity_url_code"),
    )
    .await
    else {
        return Ok(());
    };
    evaluate(
        ctx,
        services,
        new.id,
        executor,
        DestructiveActionKind::VanityChange,
    )
    .await
}
