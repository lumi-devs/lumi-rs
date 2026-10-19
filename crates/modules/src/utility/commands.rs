use poise::serenity_prelude as serenity;
use serenity::all::{EditMember, GetMessages, Message, MessageId, UserId};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};

/// Fetch and display avatar of a user or yourself.
#[poise::command(slash_command, prefix_command)]
pub async fn avatar(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let target_id = user.unwrap_or_else(|| ctx.author().id);
    let target_user = target_id.to_user(ctx.http()).await?;

    let avatar_url = target_user.face();
    let title = format!("🖼️ Avatar for {}", target_user.name);

    let mut opts = CardOptions::default();
    opts.thumbnail_url = Some(avatar_url.clone());

    let card = Card::lines(title, vec![format!("[Direct Image Link]({avatar_url})")])
        .kind(CardKind::Info)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Fetch and display banner of a user or yourself.
#[poise::command(slash_command, prefix_command, aliases("b"))]
pub async fn banner(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let target_id = user.unwrap_or_else(|| ctx.author().id);
    let target_user = ctx.http().get_user(target_id).await?;

    let card = if let Some(banner_url) = target_user.banner_url() {
        let title = format!("🖼️ Banner for {}", target_user.name);
        let mut opts = CardOptions::default();
        opts.thumbnail_url = Some(banner_url.clone());
        Card::lines(title, vec![format!("[Direct Image Link]({banner_url})")])
            .kind(CardKind::Info)
            .options(opts)
    } else {
        Card::lines(
            "🖼️ No Banner",
            vec![format!("<@{target_id}> has no banner set.")],
        )
        .kind(CardKind::Info)
    };

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Fetch and inspect user profile and membership information.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn whois(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let target_id = user.unwrap_or_else(|| ctx.author().id);
    let target_user = target_id.to_user(ctx.http()).await?;

    let (joined_at, nick, roles) = if let Some(guild_id) = ctx.guild_id() {
        if let Ok(member) = guild_id.member(ctx.http(), target_id).await {
            let roles: Vec<String> = member.roles.iter().map(|r| format!("<@&{}>", r)).collect();
            (member.joined_at, member.nick, roles)
        } else {
            (None, None, Vec::new())
        }
    } else {
        (None, None, Vec::new())
    };

    let title = format!("👤 User Information — {}", target_user.name);
    let mut lines = Vec::new();

    lines.push(format!("**Account**: <@{}> (`{}`)", target_user.id, target_user.id));
    lines.push(format!("**Created** at: <t:{}:F>", target_user.id.created_at().timestamp()));

    if let Some(joined) = joined_at {
        lines.push(format!("**Joined Server** at: <t:{}:F>", joined.timestamp()));
    }
    if let Some(n) = nick {
        lines.push(format!("**Nickname**: `{}`", n));
    }
    if !roles.is_empty() {
        lines.push(format!("**Roles** ({}): {}", roles.len(), roles.join(", ")));
    }

    let mut opts = CardOptions::default();
    opts.thumbnail_url = Some(target_user.face());

    let card = Card::lines(title, lines)
        .kind(CardKind::Info)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Display detailed statistics about the current guild.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn serverinfo(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;

    let (name, owner_id, member_count, channel_count, role_count, icon_url) = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        (
            guild.name.clone(),
            guild.owner_id,
            guild.member_count,
            guild.channels.len(),
            guild.roles.len(),
            guild.icon_url(),
        )
    };

    let title = format!("🏰 {}", name);
    let mut lines = Vec::new();

    lines.push(format!("**Guild ID**: `{}`", guild_id));
    lines.push(format!("**Owner**: <@{}>", owner_id));
    lines.push(format!("**Created**: <t:{}:D>", guild_id.created_at().timestamp()));
    lines.push(format!("**Members**: `{}`", member_count));
    lines.push(format!("**Channels**: `{}`", channel_count));
    lines.push(format!("**Roles**: `{}`", role_count));

    let mut opts = CardOptions::default();
    if let Some(icon) = icon_url {
        opts.thumbnail_url = Some(icon);
    }

    let card = Card::lines(title, lines)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Change or reset a member's nickname.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MANAGE_NICKNAMES")]
pub async fn nick(
    ctx: Context<'_>,
    #[description = "Member to rename"] member: UserId,
    #[description = "New nickname; omit to reset"] nickname: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;

    let new_nick = nickname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let builder = match &new_nick {
        Some(n) => EditMember::new().nickname(n.as_str()),
        None => EditMember::new().nickname(""),
    };
    guild_id.edit_member(ctx.http(), member, builder).await?;

    let card = match new_nick {
        Some(n) => Card::new(
            "✏️ Nickname Changed",
            format!("Set <@{member}>'s nickname to **{n}**."),
        )
        .kind(CardKind::Success),
        None => Card::new(
            "✏️ Nickname Reset",
            format!("Reset <@{member}>'s nickname."),
        )
        .kind(CardKind::Success),
    };

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Discord bulk delete only removes messages younger than 14 days.
const BULK_DELETE_MAX_AGE_SECS: i64 = 14 * 86_400;

/// Fetch and delete up to `amount` recent messages matching `keep`.
async fn run_purge(
    ctx: Context<'_>,
    amount: u16,
    keep: impl Fn(&Message) -> bool,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let channel_id = ctx.channel_id();
    let limit = u8::try_from(amount.clamp(1, 100)).unwrap_or(100);

    let messages = channel_id
        .messages(ctx.http(), GetMessages::new().limit(limit))
        .await?;

    let now = chrono::Utc::now().timestamp();
    let message_ids: Vec<MessageId> = messages
        .into_iter()
        .filter(|m| now - m.timestamp.unix_timestamp() < BULK_DELETE_MAX_AGE_SECS && keep(m))
        .map(|m| m.id)
        .collect();

    let mut deleted_count = 0;
    for chunk in message_ids.chunks(100) {
        channel_id
            .delete_messages(ctx.http(), chunk, Some("Purge command"))
            .await?;
        deleted_count += chunk.len();
    }

    let card = Card::new(
        "🧹 Purge Complete",
        format!("Successfully deleted `{deleted_count}` messages."),
    )
    .kind(CardKind::Success);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

fn parse_duration_secs(input: &str) -> Option<i64> {
    let input = input.trim();
    let idx = input.find(|c: char| !c.is_ascii_digit())?;
    let num: i64 = input[..idx].parse().ok()?;
    if num <= 0 {
        return None;
    }
    let unit = input[idx..].to_lowercase();
    match unit.as_str() {
        "m" | "min" => Some(num * 60),
        "h" | "hr" => Some(num * 3600),
        "d" | "day" => Some(num * 86_400),
        _ => None,
    }
}

/// Bulk delete messages in this channel, with optional filters.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_MESSAGES",
    subcommands(
        "purge_messages",
        "purge_user",
        "purge_bots",
        "purge_links",
        "purge_regex",
        "purge_duration"
    )
)]
pub async fn purge(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "🧹 Purge",
        "Choose a filter: `messages`, `user`, `bots`, `links`, `regex`, or `duration`.",
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Delete the most recent messages.
#[poise::command(slash_command, prefix_command, rename = "messages", guild_only)]
pub async fn purge_messages(
    ctx: Context<'_>,
    #[description = "Number of messages to delete (1-500)"] amount: u16,
) -> Result<(), Error> {
    run_purge(ctx, amount, |_| true).await
}

/// Delete recent messages from a specific member.
#[poise::command(slash_command, prefix_command, rename = "user", guild_only)]
pub async fn purge_user(
    ctx: Context<'_>,
    #[description = "Member to target"] user: UserId,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    run_purge(ctx, amount.unwrap_or(100), move |m| m.author.id == user).await
}

/// Delete recent messages sent by bots.
#[poise::command(slash_command, prefix_command, rename = "bots", guild_only)]
pub async fn purge_bots(
    ctx: Context<'_>,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    run_purge(ctx, amount.unwrap_or(100), |m| m.author.bot).await
}

/// Delete recent messages containing a link.
#[poise::command(slash_command, prefix_command, rename = "links", guild_only)]
pub async fn purge_links(
    ctx: Context<'_>,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    let re = regex::Regex::new(r"https?://\S+")
        .map_err(|e| anyhow::anyhow!("link pattern: {e}"))?;
    run_purge(ctx, amount.unwrap_or(100), move |m| {
        re.is_match(m.content.as_str())
    })
    .await
}

/// Delete recent messages matching a regular expression.
#[poise::command(slash_command, prefix_command, rename = "regex", guild_only)]
pub async fn purge_regex(
    ctx: Context<'_>,
    #[description = "Regular expression to test message content against"] pattern: String,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    let re = regex::Regex::new(&pattern)
        .map_err(|e| anyhow::anyhow!("Invalid pattern `{pattern}`: {e}"))?;
    run_purge(ctx, amount.unwrap_or(100), move |m| {
        re.is_match(m.content.as_str())
    })
    .await
}

/// Delete recent messages newer than a given age.
#[poise::command(slash_command, prefix_command, rename = "duration", guild_only)]
pub async fn purge_duration(
    ctx: Context<'_>,
    #[description = "How far back to reach, e.g. 10m, 2h, 1d"]
    #[autocomplete = "autocomplete_duration"]
    duration: String,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    let secs = parse_duration_secs(&duration).ok_or_else(|| {
        anyhow::anyhow!("Invalid duration `{duration}`. Try something like `10m`, `2h`, or `1d`.")
    })?;
    let cutoff = chrono::Utc::now().timestamp() - secs;
    run_purge(ctx, amount.unwrap_or(100), move |m| {
        m.timestamp.unix_timestamp() >= cutoff
    })
    .await
}

async fn autocomplete_duration<'a>(
    _ctx: Context<'a>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    const PRESETS: &[&str] = &[
        "5m", "10m", "15m", "30m", "1h", "2h", "4h", "8h", "12h", "1d", "3d", "7d", "14d",
    ];
    let choices: Vec<serenity::AutocompleteChoice> = PRESETS
        .iter()
        .filter(|p| p.starts_with(partial))
        .map(|p| serenity::AutocompleteChoice::new(*p, *p))
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

pub fn commands() -> Vec<Command> {
    vec![
        avatar(),
        whois(),
        serverinfo(),
        banner(),
        nick(),
        purge(),
    ]
}
