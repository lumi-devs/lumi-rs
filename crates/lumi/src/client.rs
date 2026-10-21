use std::sync::Arc;

use anyhow::Result;
use poise::serenity_prelude as serenity;

pub use lumi_kernel::bot::{Context, Data, Error};

/// Permit gate for guild commands. Commands absent from `permits::required_node`
/// pass through untouched; permitted ones get an ephemeral denial card.
fn command_check(ctx: Context<'_>) -> poise::BoxFuture<'_, Result<bool, Error>> {
    Box::pin(async move {
        let author_id = ctx.author().id;
        let is_owner = ctx.data().cfg.is_owner(author_id.get())
            || ctx.framework().options().owners.contains(&author_id);
        if is_owner {
            return Ok(true);
        }

        let Some(guild_id) = ctx.guild_id() else {
            return Ok(true);
        };

        let author_member = ctx.author_member().await;
        let (is_guild_owner, is_manager, role_ids, author_perms) = if let Some(guild) = ctx.guild()
        {
            let is_guild_owner = guild.owner_id == author_id;
            let member_ref = author_member
                .as_deref()
                .or_else(|| guild.members.get(&author_id));
            let author_perms = member_ref.map(|member| guild.member_permissions(member));
            let (is_manager, role_ids) = if let Some(member) = member_ref {
                let perms = guild.member_permissions(member);
                let is_manager = perms.contains(serenity::all::Permissions::ADMINISTRATOR)
                    || perms.contains(serenity::all::Permissions::MANAGE_GUILD);
                let mut roles: Vec<u64> = member.roles.iter().map(|role| role.get()).collect();
                roles.sort_by_key(|id| {
                    guild
                        .roles
                        .get(&serenity::all::RoleId::new(*id))
                        .map_or(0, |role| role.position)
                });
                roles.reverse();
                (is_manager, roles)
            } else {
                (false, Vec::new())
            };
            (is_guild_owner, is_manager, role_ids, author_perms)
        } else {
            (false, false, Vec::new(), None)
        };

        // Discord only reads `default_member_permissions` from the root
        // command, so a subcommand's requirement must be enforced here for
        // slash invocations too. Prefix invocations are never checked by
        // Discord at all.
        let required = required_member_permissions(ctx.command_tree());
        if !required.is_empty() && !author_perms.is_some_and(|has| holds_permissions(has, required))
        {
            let card = lumi_ui::Card::error(
                "Permission Denied",
                "You do not have the Discord permissions this command requires.",
            )
            .ephemeral();
            ctx.send(card.into_reply().reply()).await?;
            return Ok(false);
        }

        let is_quarantined =
            lumi_modules::security::store::panic_locks_commands(&ctx.data(), guild_id.get()).await;

        let node = ctx
            .command_tree()
            .iter()
            .rev()
            .find_map(|cmd| lumi_kernel::permits::command_node(cmd));
        let Some(node) = node else {
            return Ok(true);
        };

        let check = lumi_kernel::permits::CheckContext {
            guild_id: guild_id.get().to_string(),
            user_id: author_id.get(),
            channel_id: Some(ctx.channel_id().get()),
            role_ids: &role_ids,
            is_owner: false,
            is_guild_owner,
            is_manager,
            is_quarantined,
            node,
        };
        if ctx.data().permits.resolve(&check).allowed {
            return Ok(true);
        }

        let card = lumi_ui::Card::error(
            "Permission Denied",
            format!("You lack the required permit (`{node}`) to use this."),
        )
        .ephemeral();
        ctx.send(card.into_reply().reply()).await?;
        Ok(false)
    })
}

/// Discord only enforces the root command's `default_member_permissions`, so
/// every level in the tree contributes: a subcommand-only requirement (e.g.
/// `/bank set`) would otherwise never be checked.
fn required_member_permissions(tree: &[&lumi_kernel::bot::Command]) -> serenity::Permissions {
    tree.iter()
        .fold(serenity::Permissions::empty(), |required, command| {
            required | command.default_member_permissions
        })
}

fn holds_permissions(has: serenity::Permissions, required: serenity::Permissions) -> bool {
    required.is_empty() || has.administrator() || has.contains(required)
}

pub async fn build(mut cfg: lumi_kernel::Config) -> Result<serenity::Client> {
    // The per-shard guild/member split uses Discord's shard math, so it needs the
    // fleet total as well as this process's shard latencies.
    let mut shard_total = cfg.shard_count.map_or(1, u64::from);
    if let Ok(token) = cfg.token.parse::<serenity::Token>() {
        let http = serenity::Http::new(token);
        if cfg.shard_count.is_none() {
            match http.get_bot_gateway().await {
                Ok(gateway) => shard_total = u64::from(gateway.shards.get()),
                Err(e) => tracing::warn!(error = %e, "failed to read the recommended shard count"),
            }
        }
        match http.get_current_application_info().await {
            Ok(info) => {
                if let Some(owner) = info.owner {
                    let id = owner.id.get();
                    if !cfg.owner_ids.contains(&id) {
                        tracing::info!(owner_id = id, "resolved application owner from Discord");
                        cfg.owner_ids.push(id);
                    }
                }
                if let Some(team) = info.team {
                    let team_owner = team.owner_user_id.get();
                    if !cfg.owner_ids.contains(&team_owner) {
                        tracing::info!(owner_id = team_owner, "resolved team owner from Discord");
                        cfg.owner_ids.push(team_owner);
                    }
                    for member in team.members {
                        if member.membership_state == serenity::MembershipState::Accepted {
                            let id = member.user.id.get();
                            if !cfg.owner_ids.contains(&id) {
                                tracing::info!(
                                    member_id = id,
                                    "resolved team member owner from Discord"
                                );
                                cfg.owner_ids.push(id);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to resolve application owner from Discord HTTP");
            }
        }
    }

    let services = crate::runtime::services(cfg).await?;

    let commands = crate::commands::all_from_registry(&services.registry);

    let owners: std::collections::HashSet<serenity::UserId> = services
        .cfg
        .owner_ids
        .iter()
        .copied()
        .map(serenity::UserId::new)
        .collect();

    let framework = poise::Framework::new(poise::FrameworkOptions {
        commands,
        command_check: Some(command_check),
        skip_checks_for_owners: true,
        owners,
        initialize_owners: true,
        on_error: |error| Box::pin(crate::errors::on_error(error)),
        reply_callback: Some(|ctx, reply| {
            if let poise::Context::Prefix(_) = ctx {
                reply.reply(true)
            } else {
                reply
            }
        }),
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(",".into()),
            mention_as_prefix: true,
            dynamic_prefix: Some(|ctx| {
                Box::pin(async move {
                    let data = ctx.framework.user_data();
                    let db = &data.db;
                    let guild_id = ctx.guild_id.map(serenity::all::GuildId::get);
                    let p = if let Some(guild_id) = guild_id {
                        lumi_kernel::prefix::get(&data.cache, db, guild_id)
                            .await
                            .unwrap_or_else(|_| ",".to_string())
                    } else {
                        lumi_db::get_prefix(db, None)
                            .await
                            .unwrap_or_else(|_| ",".to_string())
                    };
                    Ok(Some(p.into()))
                })
            }),
            case_insensitive_commands: true,
            ..Default::default()
        },
        ..Default::default()
    });

    let mut intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;
    if services.cfg.intents_guild_members {
        intents |= serenity::GatewayIntents::GUILD_MEMBERS;
    }

    let mut cache_settings = serenity::cache::Settings::default();
    cache_settings.max_messages = 0;
    cache_settings.cache_users = false;

    let token: serenity::Token = services.cfg.token.parse()?;

    let client = serenity::ClientBuilder::new(token, intents)
        .cache_settings(cache_settings)
        .data(crate::runtime::client_data(services))
        .framework(Box::new(framework))
        .event_handler(Arc::new(crate::events::Events))
        .await?;

    let runners = Arc::clone(&client.shard_manager.runners);
    crate::ping::track_shards(move || {
        let mut shards: Vec<lumi_ui::ping::ShardInfo> = runners
            .iter()
            .map(|entry| {
                let info = entry.value().info.read();
                lumi_ui::ping::ShardInfo {
                    id: u64::from(entry.key().get()),
                    ping: info
                        .latency
                        .map_or(-1.0, |latency| latency.as_secs_f64() * 1000.0),
                    status: lumi_ui::ping::ShardStatus::from_stage(info.stage),
                    guilds: 0,
                    users: 0,
                }
            })
            .collect();
        shards.sort_unstable_by_key(|shard| shard.id);
        crate::ping::ShardSnapshot {
            shards,
            total: shard_total,
        }
    });

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::{holds_permissions, required_member_permissions};
    use lumi_kernel::bot::Command;
    use poise::serenity_prelude as serenity;

    #[test]
    fn prefix_permissions_union_the_command_tree() {
        let mut root = Command::default();
        root.name = "purge".into();
        root.default_member_permissions = serenity::Permissions::MANAGE_MESSAGES;
        let mut sub = Command::default();
        sub.name = "messages".into();

        let tree = [&root, &sub];
        assert_eq!(
            required_member_permissions(&tree),
            serenity::Permissions::MANAGE_MESSAGES
        );
        assert!(required_member_permissions(&[&sub]).is_empty());
    }

    #[test]
    fn subcommand_only_permissions_are_enforced() {
        // Discord registers `bank` with an empty default and ignores the
        // subcommand's MANAGE_GUILD, so the tree walk must pick it up.
        let mut root = Command::default();
        root.name = "bank".into();
        let mut sub = Command::default();
        sub.name = "set".into();
        sub.default_member_permissions = serenity::Permissions::MANAGE_GUILD;

        assert_eq!(
            required_member_permissions(&[&root, &sub]),
            serenity::Permissions::MANAGE_GUILD
        );
    }

    #[test]
    fn required_permissions_allow_admin_and_deny_others() {
        let required = serenity::Permissions::MANAGE_MESSAGES;
        assert!(holds_permissions(
            serenity::Permissions::MANAGE_MESSAGES,
            required
        ));
        assert!(holds_permissions(
            serenity::Permissions::ADMINISTRATOR,
            required
        ));
        assert!(!holds_permissions(serenity::Permissions::empty(), required));
        assert!(!holds_permissions(
            serenity::Permissions::SEND_MESSAGES,
            required
        ));
        assert!(holds_permissions(
            serenity::Permissions::empty(),
            serenity::Permissions::empty()
        ));
    }
}
