use poise::serenity_prelude as serenity;
use serenity::all::{CreateActionRow, CreateButton};
use std::collections::BTreeMap;

use crate::ping::collect;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};

/// Display detailed information, statistics, and architecture of the Lumi bot.
#[poise::command(slash_command, prefix_command, aliases("info", "stats", "botinfo"))]
pub async fn about(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let data = collect(ctx.data().as_ref(), ctx.serenity_context()).await;

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
            format!("{mb:.1} MB")
        }
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let boot_time_sec = now.saturating_sub((data.uptime / 1000.0) as u64);
    let host_boot_time_sec = now.saturating_sub(data.os_uptime_secs);

    let codebase = tokio::task::spawn_blocking(crate::codebase::stats)
        .await
        .unwrap_or_default();

    let services = ctx.data();
    let (postgres, redis) = tokio::join!(
        lumi_db::server_version(&services.db),
        services.cache.server_version()
    );
    let postgres = postgres.unwrap_or_else(|_| "unknown".to_string());
    let redis = redis.unwrap_or_else(|_| "unknown".to_string());

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
            "### ⚙️ __Core Architecture__\n_Lumi version_ v{}\n*Rust* v{} · *Serenity* Next · *SeaORM* 2.0\n*PostgreSQL* v{} · *Redis* v{}",
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION"),
            postgres,
            redis,
        ),
        format!(
            "### 📦 __Codebase__\n**{}** lines of Rust · **{}** modules · **{}** dependencies\n-# *Self-hosted, open source, and allergic to downtime.*",
            codebase.lines,
            ctx.data().registry.len(),
            codebase.dependencies,
        ),
    ];
    let row1 = CreateActionRow::Buttons(
        vec![
            CreateButton::new_link("https://github.com/lumi-devs/lumi-rs").label("Source Code"),
            CreateButton::new_link("https://lumi-devs.github.io/Lumi-docs").label("Documentation"),
        ]
        .into(),
    );

    let row2 = CreateActionRow::Buttons(
        vec![
            CreateButton::new_link(
                "https://lumi-devs.github.io/Lumi-docs/reference/data-and-privacy",
            )
            .label("Privacy Policy"),
            CreateButton::new_link("https://github.com/lumi-devs/Lumi/blob/main/LICENSE")
                .label("Terms of Service"),
        ]
        .into(),
    );

    let opts = CardOptions {
        hide_title: true,
        no_accent: true,
        thumbnail_url: Some(data.avatar_url),
        separator_above_action_rows: true,
        action_rows: vec![row1, row2],
        ..CardOptions::default()
    };

    let card = Card::lines("", body).kind(CardKind::Primary).options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Dynamic command directory grouped automatically by category.
#[poise::command(slash_command, prefix_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to look up"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let all_commands = ctx.framework().options().commands.as_slice();

    if let Some(query) = command {
        let found = all_commands
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&query));
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
        }
        let card = Card::new(
            "❌ Command Not Found",
            format!("No command matching `/{query}` was found."),
        )
        .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let mut categories: BTreeMap<String, Vec<(&str, &str)>> = BTreeMap::new();
    let mut claimed_commands = std::collections::HashSet::new();

    for module in ctx.data().registry.iter() {
        let meta = module.metadata();
        let cat_header = format!("{} __{}__", meta.emoji, meta.display_name);

        for cmd in module.commands() {
            claimed_commands.insert(cmd.name.clone());
            if let Some(matched) = all_commands.iter().find(|c| c.name == cmd.name) {
                let actual_desc = matched
                    .description
                    .as_deref()
                    .unwrap_or("No description provided");
                categories
                    .entry(cat_header.clone())
                    .or_default()
                    .push((&matched.name, actual_desc));
            }
        }
    }

    let core_header = "✨ __Core System__".to_string();
    for cmd in all_commands {
        if !claimed_commands.contains(&cmd.name) {
            let desc = cmd
                .description
                .as_deref()
                .unwrap_or("No description provided");
            categories
                .entry(core_header.clone())
                .or_default()
                .push((&cmd.name, desc));
        }
    }

    let mut body = vec![
        "Lumi interactive command manual. Use `/help <command>` for parameter breakdown:"
            .to_string(),
    ];

    for (cat_name, cmds) in categories {
        let mut section = format!("### {cat_name}\n");
        for (name, desc) in cmds {
            use std::fmt::Write as _;
            let _ = writeln!(section, "• `/{name}` — {desc}");
        }
        body.push(section);
    }

    let card = Card::lines("📖 Lumi Command Manual", body).kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![about(), help()]
}
