use std::collections::BTreeMap;
use poise::serenity_prelude as serenity;
use serenity::all::{CreateActionRow, CreateButton};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};
use crate::ping::collect;

/// Display detailed information, statistics, and architecture of the Lumi bot.
#[poise::command(slash_command, aliases("info", "stats", "botinfo"))]
pub async fn about(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let data = collect(0, ctx.serenity_context()).await;

    let fmt_count = |count: u64| {
        if count >= 1000 {
            format!("{:.1}K", (count as f64) / 1000.0)
        } else {
            count.to_string()
        }
    };

    let fmt_mb = |bytes: u64| {
        let mb = (bytes as f64) / (1024.0 * 1024.0);
        if mb >= 1024.0 {
            format!("{:.1} GB", mb / 1024.0)
        } else {
            format!("{:.1} MB", mb)
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let boot_time_sec = now.saturating_sub((data.uptime / 1000.0) as u64);
    let host_boot_time_sec = now.saturating_sub(data.os_uptime_secs);

    let body = vec![
        "# __Lumi__\n\nLumi is a powerful, self-hosted Discord administration platform designed for communities that demand full control and rock-solid reliability. Powered by native Rust. Lumi guarantees instant command responses, modular capability, and enterprise-grade uptime for your server. The all-in-one administration platform you host yourself - moderation, leveling, economy, and logging, engineered for reliability.".to_string(),
        format!(
            "### 📊 __Instance Statistics__\n-# {} servers · {} members · {} channels\n-# Uptime <t:{}:R> (Host Uptime <t:{}:R>)\n-# Gateway `{:.0}ms` · Memory `{}` · CPU `{:.1}%`",
            fmt_count(data.guilds),
            fmt_count(data.users),
            fmt_count(data.channels),
            boot_time_sec,
            host_boot_time_sec,
            data.ws_ping.max(0.0),
            fmt_mb(data.rss),
            data.cpu_percent
        ),
        format!(
            "### ⚙️ __Core Architecture__\n_Lumi version_ v{}\n*Rust* v{} · *Serenity* Next · *SeaORM* 2.0\n*PostgreSQL* · *Redis*",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION")
        ),
        "### 📦 __Codebase__\n**Self-hosted, open source, and allergic to downtime.**".to_string(),
    ];

    let row1 = CreateActionRow::Buttons(vec![
        CreateButton::new_link("https://github.com/lumi-devs/lumi-rs")
            .label("Source Code"),
        CreateButton::new_link("https://lumi-devs.github.io/Lumi-docs")
            .label("Documentation"),
    ].into());

    let row2 = CreateActionRow::Buttons(vec![
        CreateButton::new_link("https://lumi-devs.github.io/Lumi-docs/reference/data-and-privacy")
            .label("Privacy Policy"),
        CreateButton::new_link("https://github.com/lumi-devs/Lumi/blob/main/LICENSE")
            .label("Terms of Service"),
    ].into());

    let mut opts = CardOptions::default();
    opts.hide_title = true;
    opts.no_accent = true;
    opts.thumbnail_url = Some(data.avatar_url);
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![row1, row2];

    let card = Card::lines("", body)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Dynamic command directory grouped automatically by category.
#[poise::command(slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to look up"] command: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let all_commands = ctx.framework().options().commands.as_slice();

    if let Some(query) = command {
        let found = all_commands.iter().find(|c| c.name.eq_ignore_ascii_case(&query));
        if let Some(cmd) = found {
            let title = format!("📖 Command: /{}", cmd.name);
            let mut lines = Vec::new();
            if let Some(desc) = &cmd.description {
                lines.push(format!("**Description**: {desc}"));
            }
            if !cmd.subcommands.is_empty() {
                lines.push(String::new());
                lines.push("**Subcommands**:".to_string());
                for sub in &cmd.subcommands {
                    let sub_desc = sub.description.as_deref().unwrap_or("No description");
                    lines.push(format!("• `/{0} {1}` — {2}", cmd.name, sub.name, sub_desc));
                }
            }
            let card = Card::lines(title, lines).kind(CardKind::Info);
            ctx.send(card.into_reply().reply()).await?;
            return Ok(());
        } else {
            let card = Card::new(
                "❌ Command Not Found",
                format!("No command matching `/{query}` was found."),
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
            return Ok(());
        }
    }

    let mut categories: BTreeMap<&'static str, Vec<(&str, &str)>> = BTreeMap::new();

    for cmd in all_commands {
        let desc = cmd.description.as_deref().unwrap_or("No description provided");
        let name_str: &str = &cmd.name;
        let cat = match name_str {
            "warn" | "kick" | "ban" | "unban" | "case" | "reason" | "note" => "🛡️ Moderation",
            "panic" => "🚨 Security & Anti-Nuke",
            "tempvc" => "🎙️ Temporary Voice",
            "balance" | "daily" | "pay" => "🪙 Economy",
            "avatar" | "whois" | "serverinfo" | "purge" | "afk" => "🛠️ Utility",
            "about" | "help" | "ping" | "lumi" => "✨ Core System",
            _ => "📦 General",
        };
        categories.entry(cat).or_default().push((&cmd.name, desc));
    }

    let mut body = vec![
        "Lumi interactive command manual. Use `/help <command>` for parameter breakdown:".to_string(),
        String::new(),
    ];

    for (cat_name, cmds) in categories {
        body.push(format!("### __{cat_name}__"));
        for (name, desc) in cmds {
            body.push(format!("• `/{name}` — {desc}"));
        }
        body.push(String::new());
    }

    let card = Card::lines("📖 Lumi Command Manual", body).kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Main administrative hub and server feature overview.
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn lumi(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let (guild_name, guild_icon) = {
        let g = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        (g.name.clone(), g.icon_url())
    };

    let title = format!("🎛️ Lumi Control Hub — {}", guild_name);

    let mut body = vec![
        "Configure and review your server's active administration capabilities:".to_string(),
        String::new(),
    ];

    for m in ctx.data().registry.iter() {
        let meta = m.metadata();
        body.push(format!("### {} __{}__\n-# {}", meta.emoji, meta.display_name, meta.description));
    }

    let mut opts = CardOptions::default();
    if let Some(icon) = guild_icon {
        opts.thumbnail_url = Some(icon);
    }

    let card = Card::lines(title, body)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![about(), help(), lumi()]
}
