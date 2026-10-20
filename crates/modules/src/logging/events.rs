use poise::serenity_prelude as serenity;
use serenity::all::{
    GenericChannelId, GuildId, GuildMemberUpdateEvent, Member, Message, MessageId,
    MessageUpdateEvent, RoleId, User,
};

use super::{store, views};
use lumi_kernel::Result;

const GROUP_MESSAGE: &str = "message_log_channel_id";
const GROUP_MEMBER: &str = "member_log_channel_id";

// lumi/discord.js `Colors` values, kept identical so cards match.
const RED: u32 = 0x00ed_4245;
const GREEN: u32 = 0x0057_f287;
const GREY: u32 = 0x0095_a5a6;
const ORANGE: u32 = 0x00e6_7e22;
const BLUE: u32 = 0x0034_98db;
const PURPLE: u32 = 0x009b_59b6;
const DARK_RED: u32 = 0x0099_2d22;

/// One log entry: which toggle gates it, the channel-resolution keys, and the
/// card content. A struct keeps `emit` below the argument-count lint.
struct Entry<'a> {
    toggle: &'a str,
    event_channel: &'a str,
    group_channel: &'a str,
    color: u32,
    title: &'a str,
    lines: Vec<String>,
}

async fn emit(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    guild_id: u64,
    entry: Entry<'_>,
) -> Result<()> {
    if let Some(channel) = store::target(
        &services.cache,
        &services.db,
        guild_id,
        entry.toggle,
        entry.event_channel,
        entry.group_channel,
    )
    .await?
    {
        let card = views::log_card(entry.color, entry.title, &entry.lines);
        channel.send_message(&ctx.http, card.message()).await?;
    }
    Ok(())
}

fn member_line(user: &User) -> String {
    format!("**Member**: <@{}> ({})", user.id.get(), user.id.get())
}

/// lumi truncates message content to 450 (edits) / 900 (deletes); mirror that
/// so a single text display never exceeds Discord's limit.
fn cut(content: &str, max: usize) -> String {
    if content.chars().count() <= max {
        return content.to_string();
    }
    content.chars().take(max).collect()
}

/// Role mentions added to / removed from a member, as `<@&id>` strings.
fn role_changes(old: &[RoleId], new: &[RoleId]) -> (Vec<String>, Vec<String>) {
    let added = new
        .iter()
        .filter(|role| !old.contains(role))
        .map(|role| format!("<@&{}>", role.get()))
        .collect();
    let removed = old
        .iter()
        .filter(|role| !new.contains(role))
        .map(|role| format!("<@&{}>", role.get()))
        .collect();
    (added, removed)
}

pub async fn on_message_delete(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    channel_id: GenericChannelId,
    message_id: MessageId,
    guild_id: Option<GuildId>,
) -> Result<()> {
    let Some(guild_id) = guild_id else {
        return Ok(());
    };
    let guild = guild_id.get();
    if store::is_ignored_channel(&services.cache, &services.db, guild, channel_id.get()).await? {
        return Ok(());
    }
    emit(
        ctx,
        services,
        guild,
        Entry {
            toggle: "message_deletes",
            event_channel: "message_deletes_channel_id",
            group_channel: GROUP_MESSAGE,
            color: RED,
            title: "Message Deleted",
            lines: vec![
                format!("**Channel**: <#{}>", channel_id.get()),
                format!("**Message ID**: `{message_id}`"),
            ],
        },
    )
    .await
}

pub async fn on_message_update(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    old: Option<Message>,
    event: MessageUpdateEvent,
) -> Result<()> {
    let new = &event.message;
    let Some(guild_id) = new.guild_id else {
        return Ok(());
    };
    if new.author.bot() {
        return Ok(());
    }
    let guild = guild_id.get();
    if store::is_ignored_channel(&services.cache, &services.db, guild, new.channel_id.get()).await?
    {
        return Ok(());
    }
    let before = old
        .as_ref()
        .map(|message| message.content.to_string())
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| "(unknown)".to_string());
    let after = new.content.to_string();
    if before == after {
        return Ok(());
    }
    emit(
        ctx,
        services,
        guild,
        Entry {
            toggle: "message_edits",
            event_channel: "message_edits_channel_id",
            group_channel: GROUP_MESSAGE,
            color: ORANGE,
            title: "Message Edited",
            lines: vec![
                format!(
                    "**Author**: <@{}> ({})",
                    new.author.id.get(),
                    new.author.id.get()
                ),
                format!("**Channel**: <#{}>", new.channel_id.get()),
                format!("**Before**: {}", cut(&before, 450)),
                format!("**After**: {}", cut(&after, 450)),
                format!("[Jump to message]({})", new.link()),
            ],
        },
    )
    .await
}

pub async fn on_member_add(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    member: &Member,
) -> Result<()> {
    let created = member.user.id.created_at().unix_timestamp();
    emit(
        ctx,
        services,
        member.guild_id.get(),
        Entry {
            toggle: "member_joins",
            event_channel: "member_joins_channel_id",
            group_channel: GROUP_MEMBER,
            color: GREEN,
            title: "Member Joined",
            lines: vec![
                member_line(&member.user),
                format!("**Account created**: <t:{created}:R>"),
            ],
        },
    )
    .await
}

pub async fn on_member_remove(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    guild_id: GuildId,
    user: &User,
) -> Result<()> {
    emit(
        ctx,
        services,
        guild_id.get(),
        Entry {
            toggle: "member_leaves",
            event_channel: "member_leaves_channel_id",
            group_channel: GROUP_MEMBER,
            color: GREY,
            title: "Member Left",
            lines: vec![member_line(user)],
        },
    )
    .await
}

pub async fn on_ban_add(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    guild_id: GuildId,
    user: &User,
) -> Result<()> {
    emit(
        ctx,
        services,
        guild_id.get(),
        Entry {
            toggle: "member_bans",
            event_channel: "member_bans_channel_id",
            group_channel: GROUP_MEMBER,
            color: DARK_RED,
            title: "Member Banned",
            lines: vec![member_line(user)],
        },
    )
    .await
}

pub async fn on_ban_remove(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    guild_id: GuildId,
    user: &User,
) -> Result<()> {
    emit(
        ctx,
        services,
        guild_id.get(),
        Entry {
            toggle: "member_unbans",
            event_channel: "member_unbans_channel_id",
            group_channel: GROUP_MEMBER,
            color: GREEN,
            title: "Member Unbanned",
            lines: vec![member_line(user)],
        },
    )
    .await
}

pub async fn on_member_update(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    old: Option<Member>,
    event: GuildMemberUpdateEvent,
) -> Result<()> {
    // Without the cached old member we cannot diff nickname or roles, matching
    // lumi's `oldMember.partial` guard.
    let Some(old) = old else {
        return Ok(());
    };
    let guild = event.guild_id.get();
    let user = &event.user;

    if old.nick.as_deref() != event.nick.as_deref() {
        emit(
            ctx,
            services,
            guild,
            Entry {
                toggle: "nickname_changes",
                event_channel: "nickname_changes_channel_id",
                group_channel: GROUP_MEMBER,
                color: BLUE,
                title: "Nickname Changed",
                lines: vec![
                    member_line(user),
                    format!("**Before**: {}", old.nick.as_deref().unwrap_or("*none*")),
                    format!("**After**: {}", event.nick.as_deref().unwrap_or("*none*")),
                ],
            },
        )
        .await?;
    }

    let (added, removed) = role_changes(&old.roles, &event.roles);
    if !added.is_empty() || !removed.is_empty() {
        let mut lines = vec![member_line(user)];
        if !added.is_empty() {
            lines.push(format!("**Added**: {}", added.join(" ")));
        }
        if !removed.is_empty() {
            lines.push(format!("**Removed**: {}", removed.join(" ")));
        }
        emit(
            ctx,
            services,
            guild,
            Entry {
                toggle: "role_changes",
                event_channel: "role_changes_channel_id",
                group_channel: GROUP_MEMBER,
                color: PURPLE,
                title: "Roles Updated",
                lines,
            },
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cut, role_changes};
    use serenity::all::RoleId;

    #[test]
    fn role_changes_diffs_add_and_remove() {
        let (added, removed) = role_changes(
            &[RoleId::new(1), RoleId::new(2)],
            &[RoleId::new(2), RoleId::new(3)],
        );
        assert_eq!(added, vec!["<@&3>".to_string()]);
        assert_eq!(removed, vec!["<@&1>".to_string()]);
    }

    #[test]
    fn cut_respects_char_boundaries() {
        assert_eq!(cut("hello", 10), "hello");
        assert_eq!(cut("hello", 3), "hel");
    }
}
