use std::collections::BTreeMap;
use poise::serenity_prelude as serenity;
use serenity::all::{CreateActionRow, CreateButton};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};
use crate::ping::collect;

/// System information, hardware metrics, and bot architecture overview.
#[poise::command(slash_command)]
pub async fn about(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let data = collect(0, ctx.serenity_context()).await;
    let title = "✨ About Lumi";

    let body = vec![
        "### __Lumi (Rust Engine)__".to_string(),
        "Lumi is a native, self-hosted Discord administration platform written in **Rust** designed to effortlessly scale across millions of communities with microsecond command latency.".to_string(),
        String::new(),
        "### __Core Architecture__".to_string(),
        format!("• **Runtime**: `{}` on `{}` (`{}`)", data.runtime, data.platform, data.arch),
        format!("• **Discord Stack**: {}", data.discord_lib),
        format!("• **Database Engine**: {}", data.orm_lib),
        "• **Components Engine**: Discord Components V2 (Native Cards, zero legacy embeds)".to_string(),
        "• **Global Allocator**: mimalloc (high concurrency, low fragmentation)".to_string(),
        String::new(),
        "### __Cluster Status__".to_string(),
        format!("• **Cached Guilds**: `{}`", data.guilds),
        format!("• **Cached Users**: `{}`", data.users),
        format!("• **Active Channels**: `{}`", data.channels),
        format!("• **Memory Footprint**: {:.1} MB", (data.rss as f64) / (1024.0 * 1024.0)),
        format!("• **CPU Cores**: {} (Load: {:.2})", data.cpu_cores, data.load_avg[0]),
    ];

    let row = CreateActionRow::Buttons(vec![
        CreateButton::new_link("https://github.com/lumi-devs/lumi-rs")
            .label("GitHub Source")
            .emoji('📦'),
    ].into());

    let mut opts = CardOptions::default();
    opts.thumbnail_url = Some(data.avatar_url);
    opts.action_rows = vec![row];

    let card = Card::lines(title, body)
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

    // If specific command requested, show its detailed documentation
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

    // Dynamic categorisation based on module origin and command purpose
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

/// Main administrative hub and cluster health overview.
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

    let modules = ctx.data().registry.iter().map(|m| {
        let meta = m.metadata();
        format!("• {} **{}** (v{}) — {}", meta.emoji, meta.display_name, meta.version, meta.description)
    }).collect::<Vec<_>>();

    let mut body = vec![
        "Server administration overview and active engine subsystems:".to_string(),
        String::new(),
        "### __Active Modules__".to_string(),
    ];
    body.extend(modules);

    body.push(String::new());
    body.push("### __Integrated Kernel Subsystems__".to_string());
    body.push("• 🛡️ **Anti-Nuke**: Active (In-memory sliding window rate trackers)".to_string());
    body.push("• 🔍 **Content Filter**: Active (Zero-allocation Aho-Corasick & regex matcher)".to_string());
    body.push("• 🔒 **Panic Lockdown**: Configured (`/panic on|off|status`)".to_string());

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
