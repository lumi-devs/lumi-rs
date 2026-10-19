use poise::serenity_prelude as serenity;
use serenity::all::{
    ChannelType, CreateButton, EditMember, GetMessages, Message, MessageId, Permissions, UserId,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::panels::action_row_of;
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

    let (joined_at, nick, premium_since, role_ids, key_perms_str, accent_color) = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        let member_opt = guild.members.get(&target_id);

        if let Some(member) = member_opt {
            let joined_at = member.joined_at;
            let nick = member.nick.clone();
            let premium_since = member.premium_since;

            let mut sorted_roles: Vec<_> = member
                .roles
                .iter()
                .filter(|r| r.get() != guild.id.get())
                .filter_map(|r| guild.roles.get(r).map(|role| (role.position, role.id, role.colour)))
                .collect();
            sorted_roles.sort_by_key(|r| std::cmp::Reverse(r.0));

            let accent_color = sorted_roles.iter().find(|r| r.2.0 != 0).map(|r| r.2);
            let role_ids: Vec<_> = sorted_roles.into_iter().map(|r| r.1).collect();

            let permissions = guild.member_permissions(member);
            let key_perms_str = if permissions.contains(Permissions::ADMINISTRATOR) {
                "Administrator".to_string()
            } else {
                let mut list = Vec::new();
                if permissions.contains(Permissions::MANAGE_GUILD) {
                    list.push("Manage Server");
                }
                if permissions.contains(Permissions::MANAGE_ROLES) {
                    list.push("Manage Roles");
                }
                if permissions.contains(Permissions::MANAGE_CHANNELS) {
                    list.push("Manage Channels");
                }
                if permissions.contains(Permissions::BAN_MEMBERS) {
                    list.push("Ban Members");
                }
                if permissions.contains(Permissions::KICK_MEMBERS) {
                    list.push("Kick Members");
                }
                if permissions.contains(Permissions::MANAGE_MESSAGES) {
                    list.push("Manage Messages");
                }
                if permissions.contains(Permissions::MENTION_EVERYONE) {
                    list.push("Mention Everyone");
                }
                if list.is_empty() {
                    "None".to_string()
                } else {
                    list.join(", ")
                }
            };
            (joined_at, nick, premium_since, role_ids, key_perms_str, accent_color)
        } else {
            (None, None, None, Vec::new(), "None".to_string(), None)
        }
    };

    let mut lines = Vec::new();
    lines.push(format!("**Account**: <@{target_id}> (`{target_id}`)"));
    lines.push(format!(
        "**Created**: <t:{}:F> (<t:{}:R>)",
        target_id.created_at().timestamp(),
        target_id.created_at().timestamp()
    ));

    if let Some(joined) = joined_at {
        lines.push(format!(
            "**Joined Server**: <t:{}:F> (<t:{}:R>)",
            joined.timestamp(),
            joined.timestamp()
        ));
    }
    if let Some(ref n) = nick {
        lines.push(format!("**Nickname**: `{n}`"));
    }
    if let Some(ps) = premium_since {
        lines.push(format!("💎 **Server Booster** since: <t:{}:R>", ps.timestamp()));
    }

    if !role_ids.is_empty() {
        let role_mentions: Vec<String> = role_ids.iter().map(|id| format!("<@&{id}>")).collect();
        let mut roles_str = String::new();
        let mut shown = 0;
        for r in &role_mentions {
            if roles_str.len() + r.len() > 600 {
                break;
            }
            if !roles_str.is_empty() {
                roles_str.push(' ');
            }
            roles_str.push_str(r);
            shown += 1;
        }
        if shown < role_ids.len() {
            roles_str.push_str(&format!(" ...+{} more", role_ids.len() - shown));
        }
        lines.push(format!("**Roles** ({}): {roles_str}", role_ids.len()));
    }

    lines.push(format!("**Key Permissions**: {key_perms_str}"));

    let mut buttons = vec![CreateButton::new_link(format!("discord://-/users/{target_id}"))
        .label("Profile")
        .emoji('👤')];

    let avatar_url = target_user.face();
    buttons.push(
        CreateButton::new_link(avatar_url.clone())
            .label("Avatar")
            .emoji('🖼'),
    );

    let mut opts = CardOptions::default();
    opts.thumbnail_url = Some(avatar_url);
    opts.action_rows = vec![action_row_of(buttons)];

    let bot_tag = if target_user.bot() { " [BOT]" } else { "" };
    let title = format!("👤 User Information — {}{bot_tag}", target_user.name);

    let mut card = Card::lines(title, lines)
        .kind(CardKind::Info)
        .options(opts);

    if let Some(color) = accent_color {
        card = card.accent(Some(color));
    }

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Display detailed statistics about the current guild.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn serverinfo(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let (name, owner_id, guild_created, guild_id_val, member_count, boost_tier, boost_count, text_channels, voice_channels, category_channels, total_channels, role_count, emoji_count, verification, icon_opt) = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        let text = guild.channels.iter().filter(|c| c.base.kind == ChannelType::Text || c.base.kind == ChannelType::News).count();
        let voice = guild.channels.iter().filter(|c| c.base.kind == ChannelType::Voice || c.base.kind == ChannelType::Stage).count();
        let cat = guild.channels.iter().filter(|c| c.base.kind == ChannelType::Category).count();
        let total_c = guild.channels.len();
        let b_count = guild.premium_subscription_count.map_or(0, |c| c.get());
        let b_tier = format!("{:?}", guild.premium_tier);
        let verif = format!("{:?}", guild.verification_level);
        (
            guild.name.clone(),
            guild.owner_id,
            guild.id.created_at().timestamp(),
            guild.id,
            guild.member_count,
            b_tier,
            b_count,
            text,
            voice,
            cat,
            total_c,
            guild.roles.len(),
            guild.emojis.len(),
            verif,
            guild.icon_url(),
        )
    };

    let title = format!("🏰 {name}");
    let mut lines = Vec::new();

    lines.push(format!("**Owner**: <@{owner_id}> (`{owner_id}`)"));
    lines.push(format!(
        "**Created**: <t:{guild_created}:F> (<t:{guild_created}:R>)"
    ));
    lines.push(format!("**Guild ID**: `{guild_id_val}`"));

    lines.push("### 👥 Members & Boosts".to_string());
    lines.push(format!("**Total Members**: `{member_count}`"));
    lines.push(format!("**Server Boosts**: `{boost_count}` (Tier {boost_tier})"));

    lines.push("### 🌐 Channels".to_string());
    lines.push(format!(
        "**Text**: `{text_channels}` · **Voice**: `{voice_channels}` · **Categories**: `{category_channels}` · **Total**: `{total_channels}`"
    ));

    lines.push("### ⚙️ Server Configuration".to_string());
    lines.push(format!(
        "**Roles**: `{role_count}` · **Emojis**: `{emoji_count}` · **Verification**: `{verification}`"
    ));

    let mut buttons = Vec::new();
    let mut opts = CardOptions::default();
    if let Some(icon) = icon_opt {
        opts.thumbnail_url = Some(icon.clone());
        buttons.push(CreateButton::new_link(icon).label("Server Icon").emoji('🖼'));
    }
    if !buttons.is_empty() {
        opts.action_rows = vec![action_row_of(buttons)];
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
    run_purge(ctx, amount.unwrap_or(100), |m| m.author.bot()).await
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
