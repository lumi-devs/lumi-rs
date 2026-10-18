use poise::serenity_prelude as serenity;
use serenity::all::{GetMessages, MessageId, UserId};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};

/// Fetch and display avatar of a user or yourself.
#[poise::command(slash_command)]
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

/// Fetch and inspect user profile and membership information.
#[poise::command(slash_command, guild_only)]
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
#[poise::command(slash_command, guild_only)]
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

/// Bulk delete up to 100 messages in the current channel.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn purge(
    ctx: Context<'_>,
    #[description = "Number of messages to delete (1-100)"] amount: u8,
) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let channel_id = ctx.channel_id();

    let count = amount.clamp(1, 100);
    let messages = channel_id
        .messages(ctx.http(), GetMessages::new().limit(count))
        .await?;

    let message_ids: Vec<MessageId> = messages.into_iter().map(|m| m.id).collect();
    let deleted_count = message_ids.len();

    channel_id.delete_messages(ctx.http(), &message_ids, Some("Purge command")).await?;

    let card = Card::lines(
        "🧹 Purge Complete",
        vec![format!("Successfully deleted `{}` messages.", deleted_count)],
    )
    .kind(CardKind::Success);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![avatar(), whois(), serverinfo(), purge()]
}
