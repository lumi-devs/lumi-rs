use std::fmt::Write;

use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelType, Colour, ComponentInteraction, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateMediaGallery,
    CreateMediaGalleryItem, CreateUnfurledMediaItem, EditMember, GetMessages, Message,
    MessageFlags, MessageId, Permissions, RoleId, User, UserId,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_kernel::module_state;
use lumi_ui::panels::action_row_of;
use lumi_ui::{Card, CardKind, CardOptions};

use crate::shared::{
    claim_cooldown, cooldown_key, format_seconds, require_guild, secs_ceil, truncate_chars,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MediaType {
    Avatar,
    Banner,
}

impl MediaType {
    const fn label(self) -> &'static str {
        match self {
            Self::Avatar => "Avatar",
            Self::Banner => "Banner",
        }
    }

    const fn key(self) -> &'static str {
        match self {
            Self::Avatar => "avatar",
            Self::Banner => "banner",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "avatar" => Some(Self::Avatar),
            "banner" => Some(Self::Banner),
            _ => None,
        }
    }

    fn url(self, user: &User) -> Option<String> {
        match self {
            Self::Avatar => user.avatar_url(),
            Self::Banner => user.banner_url(),
        }
    }
}

fn gallery(url: String) -> CreateMediaGallery<'static> {
    CreateMediaGallery::new(vec![CreateMediaGalleryItem::new(
        CreateUnfurledMediaItem::new(url),
    )])
}

fn build_media_card(user: &User, kind: MediaType, show_media: bool, ephemeral: bool) -> Card {
    let url = kind.url(user);
    let mut buttons = Vec::new();
    let mut opts = CardOptions {
        thumbnail_url: url.clone(),
        ..CardOptions::default()
    };

    if let Some(u) = &url
        && show_media
    {
        opts.media_gallery = Some(gallery(u.clone()));
        buttons.push(CreateButton::new_link(u.clone()).label(format!("Open {}", kind.label())));
    } else if url.is_some() {
        buttons.push(
            CreateButton::new(format!("media:view:{}:{}", kind.key(), user.id))
                .style(ButtonStyle::Primary)
                .label(format!("View {}", kind.label())),
        );
    }
    if !buttons.is_empty() {
        opts.action_rows = vec![action_row_of(buttons)];
    }

    let display = user.global_name.as_deref().unwrap_or(&user.name);
    let mut card = Card::lines(format!("🖼️ {} — {}", kind.label(), display), vec![])
        .kind(CardKind::Info)
        .options(opts);
    if ephemeral {
        card = card.ephemeral();
    }
    card
}

/// Fetch and display avatar of a user or yourself.
#[poise::command(slash_command, prefix_command, guild_only, aliases("av"))]
pub async fn avatar(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: Option<UserId>,
) -> Result<(), Error> {
    media_request(ctx, user, MediaType::Avatar).await
}

/// Fetch and display banner of a user or yourself.
#[poise::command(slash_command, prefix_command, guild_only, aliases("b"))]
pub async fn banner(
    ctx: Context<'_>,
    #[description = "User to inspect"] user: Option<UserId>,
) -> Result<(), Error> {
    media_request(ctx, user, MediaType::Banner).await
}

async fn media_request(
    ctx: Context<'_>,
    user: Option<UserId>,
    kind: MediaType,
) -> Result<(), Error> {
    let target_id = user.unwrap_or_else(|| ctx.author().id);
    let is_self = target_id == ctx.author().id;
    let guild_id = require_guild(ctx)?.get();

    let cooldown_seconds = module_state::config_cached_value(
        &ctx.data().cache,
        &ctx.data().db,
        guild_id,
        "utility",
        "cooldown_seconds",
    )
    .await
    .and_then(|v| module_state::parse_id(&v))
    .unwrap_or(10);
    let key = cooldown_key("media", guild_id, ctx.author().id.get());
    if let Some(remaining) = claim_cooldown(
        &ctx.data().cache,
        &key,
        cooldown_seconds.saturating_mul(1000),
    )
    .await?
    {
        let card = Card::new(
            "⏳ Slow Down",
            format!("Try again in **{}**.", format_seconds(secs_ceil(remaining))),
        )
        .kind(CardKind::Error)
        .ephemeral();
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let target_user = target_id.to_user(ctx.http()).await?;
    let card = build_media_card(&target_user, kind, is_self, !is_self);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Re-render a media card with the image revealed (the "View" button).
pub async fn handle_component(
    ctx: &serenity::Context,
    _services: &lumi_kernel::Services,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let mut parts = interaction.data.custom_id.split(':');
    if parts.next() != Some("media") || parts.next() != Some("view") {
        return Ok(());
    }
    let Some(kind) = parts.next().and_then(MediaType::parse) else {
        return Ok(());
    };
    let Some(user_id) = parts.next().and_then(|id| id.parse::<u64>().ok()) else {
        return Ok(());
    };

    let user = ctx.http.get_user(UserId::new(user_id)).await?;
    let card = build_media_card(&user, kind, true, true);

    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.into_reply().components)
                    .allowed_mentions(serenity::all::CreateAllowedMentions::new())
                    .flags(MessageFlags::IS_COMPONENTS_V2),
            ),
        )
        .await?;
    Ok(())
}

struct MemberInfo {
    joined_at: Option<serenity::Timestamp>,
    nick: Option<String>,
    premium_since: Option<serenity::Timestamp>,
    role_ids: Vec<RoleId>,
    key_perms: String,
    accent_color: Option<Colour>,
}

fn key_permissions(permissions: Permissions) -> String {
    if permissions.contains(Permissions::ADMINISTRATOR) {
        return "Administrator".to_string();
    }
    let mut list = Vec::new();
    for (permission, label) in [
        (Permissions::MANAGE_GUILD, "Manage Server"),
        (Permissions::MANAGE_ROLES, "Manage Roles"),
        (Permissions::MANAGE_CHANNELS, "Manage Channels"),
        (Permissions::BAN_MEMBERS, "Ban Members"),
        (Permissions::KICK_MEMBERS, "Kick Members"),
        (Permissions::MANAGE_MESSAGES, "Manage Messages"),
        (Permissions::MENTION_EVERYONE, "Mention Everyone"),
    ] {
        if permissions.contains(permission) {
            list.push(label);
        }
    }
    if list.is_empty() {
        "None".to_string()
    } else {
        list.join(", ")
    }
}

fn member_info(guild: &serenity::Guild, target_id: UserId) -> MemberInfo {
    let Some(member) = guild.members.get(&target_id) else {
        return MemberInfo {
            joined_at: None,
            nick: None,
            premium_since: None,
            role_ids: Vec::new(),
            key_perms: "None".to_string(),
            accent_color: None,
        };
    };
    let mut sorted_roles: Vec<_> = member
        .roles
        .iter()
        .filter(|r| r.get() != guild.id.get())
        .filter_map(|r| {
            guild
                .roles
                .get(r)
                .map(|role| (role.position, role.id, role.colour))
        })
        .collect();
    sorted_roles.sort_by_key(|r| std::cmp::Reverse(r.0));
    let accent_color = sorted_roles.iter().find(|r| r.2.0 != 0).map(|r| r.2);
    MemberInfo {
        joined_at: member.joined_at,
        nick: member.nick.as_deref().map(ToString::to_string),
        premium_since: member.premium_since,
        role_ids: sorted_roles.into_iter().map(|r| r.1).collect(),
        key_perms: key_permissions(guild.member_permissions(member)),
        accent_color,
    }
}

fn roles_summary(role_ids: &[RoleId]) -> Option<String> {
    if role_ids.is_empty() {
        return None;
    }
    let mut roles_str = String::new();
    let mut shown = 0;
    for id in role_ids {
        let mention = format!("<@&{id}>");
        if roles_str.len() + mention.len() > 600 {
            break;
        }
        if !roles_str.is_empty() {
            roles_str.push(' ');
        }
        roles_str.push_str(&mention);
        shown += 1;
    }
    if shown < role_ids.len() {
        let _ = write!(roles_str, " ...+{} more", role_ids.len() - shown);
    }
    Some(format!("**Roles** ({}): {roles_str}", role_ids.len()))
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
    let info = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        member_info(&guild, target_id)
    };

    let mut lines = vec![
        format!("**Account**: <@{target_id}> (`{target_id}`)"),
        format!(
            "**Created**: <t:{}:F> (<t:{}:R>)",
            target_id.created_at().timestamp(),
            target_id.created_at().timestamp()
        ),
    ];
    if let Some(joined) = info.joined_at {
        lines.push(format!(
            "**Joined Server**: <t:{}:F> (<t:{}:R>)",
            joined.timestamp(),
            joined.timestamp()
        ));
    }
    if let Some(n) = &info.nick {
        lines.push(format!("**Nickname**: `{n}`"));
    }
    if let Some(ps) = info.premium_since {
        lines.push(format!(
            "💎 **Server Booster** since: <t:{}:R>",
            ps.timestamp()
        ));
    }
    if let Some(roles) = roles_summary(&info.role_ids) {
        lines.push(roles);
    }
    lines.push(format!("**Key Permissions**: {}", info.key_perms));

    let avatar_url = target_user.face();
    let buttons = vec![
        CreateButton::new_link(format!("discord://-/users/{target_id}"))
            .label("Profile")
            .emoji('👤'),
        CreateButton::new_link(avatar_url.clone())
            .label("Avatar")
            .emoji('🖼'),
    ];
    let opts = CardOptions {
        thumbnail_url: Some(avatar_url),
        action_rows: vec![action_row_of(buttons)],
        ..CardOptions::default()
    };

    let bot_tag = if target_user.bot() { " [BOT]" } else { "" };
    let title = format!("👤 User Information — {}{bot_tag}", target_user.name);

    let mut card = Card::lines(title, lines).kind(CardKind::Info).options(opts);

    if let Some(color) = info.accent_color {
        card = card.accent(Some(color));
    }

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Display detailed statistics about the current guild.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn serverinfo(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let (title, lines, icon) = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        let count = |kind: ChannelType| {
            guild
                .channels
                .iter()
                .filter(|channel| channel.base.kind == kind)
                .count()
        };
        let text = count(ChannelType::Text) + count(ChannelType::News);
        let voice = count(ChannelType::Voice) + count(ChannelType::Stage);
        let categories = count(ChannelType::Category);
        let created = guild.id.created_at().timestamp();
        let boosts = guild.premium_subscription_count.map_or(0, |c| c.get());

        let lines = vec![
            format!("**Owner**: <@{}> (`{}`)", guild.owner_id, guild.owner_id),
            format!("**Created**: <t:{created}:F> (<t:{created}:R>)"),
            format!("**Guild ID**: `{}`", guild.id),
            "### 👥 Members & Boosts".to_string(),
            format!("**Total Members**: `{}`", guild.member_count),
            format!(
                "**Server Boosts**: `{boosts}` (Tier {:?})",
                guild.premium_tier
            ),
            "### 🌐 Channels".to_string(),
            format!(
                "**Text**: `{text}` · **Voice**: `{voice}` · **Categories**: `{categories}` · **Total**: `{}`",
                guild.channels.len()
            ),
            "### ⚙️ Server Configuration".to_string(),
            format!(
                "**Roles**: `{}` · **Emojis**: `{}` · **Verification**: {:?}",
                guild.roles.len(),
                guild.emojis.len(),
                guild.verification_level
            ),
        ];
        (format!("🏰 {}", guild.name), lines, guild.icon_url())
    };

    let mut buttons = Vec::new();
    let thumbnail_url = icon.inspect(|icon| {
        buttons.push(
            CreateButton::new_link(icon.clone())
                .label("Server Icon")
                .emoji('🖼'),
        );
    });
    let opts = CardOptions {
        thumbnail_url,
        action_rows: if buttons.is_empty() {
            Vec::new()
        } else {
            vec![action_row_of(buttons)]
        },
        ..CardOptions::default()
    };

    let card = Card::lines(title, lines)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Change or reset a member's nickname.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_NICKNAMES"
)]
pub async fn nick(
    ctx: Context<'_>,
    #[description = "Member to rename"] member: UserId,
    #[description = "New nickname; omit to reset"] nickname: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?;

    let new_nick = nickname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 32));

    let builder = EditMember::new().nickname(new_nick.as_deref().unwrap_or(""));
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
        .filter(|m| {
            now.saturating_sub(m.timestamp.unix_timestamp()) < BULK_DELETE_MAX_AGE_SECS && keep(m)
        })
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
    let unit = &input[idx..];
    if unit.eq_ignore_ascii_case("s") || unit.eq_ignore_ascii_case("sec") {
        Some(num)
    } else if unit.eq_ignore_ascii_case("m") || unit.eq_ignore_ascii_case("min") {
        num.checked_mul(60)
    } else if unit.eq_ignore_ascii_case("h") || unit.eq_ignore_ascii_case("hr") {
        num.checked_mul(3_600)
    } else if unit.eq_ignore_ascii_case("d") || unit.eq_ignore_ascii_case("day") {
        num.checked_mul(86_400)
    } else if unit.eq_ignore_ascii_case("w") || unit.eq_ignore_ascii_case("week") {
        num.checked_mul(604_800)
    } else {
        None
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
    #[description = "Number of messages to scan (1-100)"] amount: u16,
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
    run_purge(ctx, amount.unwrap_or(100), |m| m.author.bot()).await
}

/// Delete recent messages containing a link.
#[poise::command(slash_command, prefix_command, rename = "links", guild_only)]
pub async fn purge_links(
    ctx: Context<'_>,
    #[description = "How many matching messages to delete (default 100)"] amount: Option<u16>,
) -> Result<(), Error> {
    let re =
        regex::Regex::new(r"https?://\S+").map_err(|e| anyhow::anyhow!("link pattern: {e}"))?;
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
        anyhow::anyhow!("Invalid duration `{duration}`. Try `10m`, `2h`, or `1d`.")
    })?;
    let secs = secs.min(BULK_DELETE_MAX_AGE_SECS);
    let cutoff = chrono::Utc::now().timestamp().saturating_sub(secs);
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
    tokio::task::yield_now().await;
    let choices: Vec<serenity::AutocompleteChoice> = PRESETS
        .iter()
        .filter(|p| p.starts_with(partial))
        .map(|p| serenity::AutocompleteChoice::new(*p, *p))
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

#[must_use]
pub fn commands() -> Vec<Command> {
    vec![avatar(), whois(), serverinfo(), banner(), nick(), purge()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_secs() {
        assert_eq!(parse_duration_secs("30s"), Some(30));
        assert_eq!(parse_duration_secs("10m"), Some(600));
        assert_eq!(parse_duration_secs("2h"), Some(7200));
        assert_eq!(parse_duration_secs("1d"), Some(86400));
        assert_eq!(parse_duration_secs("1w"), Some(604_800));
        assert_eq!(parse_duration_secs("0m"), None);
        assert_eq!(parse_duration_secs("invalid"), None);
    }

    #[test]
    fn test_media_type() {
        assert_eq!(MediaType::parse("avatar"), Some(MediaType::Avatar));
        assert_eq!(MediaType::parse("banner"), Some(MediaType::Banner));
        assert_eq!(MediaType::parse("other"), None);
        assert_eq!(MediaType::Avatar.label(), "Avatar");
        assert_eq!(MediaType::Banner.label(), "Banner");
        assert_eq!(MediaType::Avatar.key(), "avatar");
        assert_eq!(MediaType::Banner.key(), "banner");
    }
}
