//! DM relay, channel say and nickname sanitising commands.

use serenity::all::{CreateAllowedMentions, CreateMessage, EditMember, GenericChannelId, UserId};

use lumi_kernel::bot::{Context, Error};
use lumi_ui::{Card, CardKind};

use super::{require_guild, send_card};

/// Relay a direct message through the bot to a user.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn dm(
    ctx: Context<'_>,
    #[description = "User to message"] user: UserId,
    #[description = "Message to send"]
    #[max_length = 2000]
    message: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_name = ctx
        .guild()
        .map_or_else(|| "the server".to_string(), |guild| guild.name.to_string());

    let card = Card::lines(format!("📨 Message from {guild_name} staff"), vec![message])
        .kind(CardKind::Info);
    if user
        .dm(ctx.http(), card.into_reply().message())
        .await
        .is_err()
    {
        return send_card(
            ctx,
            Card::lines(
                "❌ Could Not Send",
                vec![format!(
                    "<@{}> has DMs closed or has blocked the bot.",
                    user.get()
                )],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    send_card(
        ctx,
        Card::lines(
            "✅ Message Sent",
            vec![format!("Sent your message to <@{}>.", user.get())],
        )
        .kind(CardKind::Success),
    )
    .await
}

/// Relay a message through the bot into a channel.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_MESSAGES",
    required_bot_permissions = "SEND_MESSAGES"
)]
pub async fn say(
    ctx: Context<'_>,
    #[description = "Message to send"]
    #[max_length = 2000]
    message: String,
    #[description = "Channel to send in (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let channel_id = channel.unwrap_or_else(|| ctx.channel_id());

    channel_id
        .send_message(
            ctx.http(),
            CreateMessage::new()
                .content(message)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await?;

    send_card(
        ctx,
        Card::lines(
            "✅ Message Sent",
            vec![format!("Relayed your message to <#{}>.", channel_id.get())],
        )
        .kind(CardKind::Success),
    )
    .await
}

fn strip_hoisting(name: &str) -> String {
    name.trim_start_matches(|character: char| !character.is_alphanumeric())
        .trim()
        .to_string()
}

/// Remove hoisting characters from a member's nickname.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_NICKNAMES",
    required_bot_permissions = "MANAGE_NICKNAMES"
)]
pub async fn sanitize(
    ctx: Context<'_>,
    #[description = "Member to sanitize"] member: UserId,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = require_guild(ctx)?;

    let guild_member = guild.member(ctx.http(), member).await?;
    let current = guild_member
        .nick
        .as_deref()
        .unwrap_or(&guild_member.user.name)
        .to_string();
    let sanitized = strip_hoisting(&current);

    if sanitized == current {
        return send_card(
            ctx,
            Card::lines(
                "✅ Nothing to Sanitize",
                vec![format!(
                    "<@{}>'s nickname has no hoisting characters.",
                    member.get()
                )],
            )
            .kind(CardKind::Info),
        )
        .await;
    }

    guild
        .edit_member(ctx.http(), member, EditMember::new().nickname(sanitized))
        .await?;

    send_card(
        ctx,
        Card::lines(
            "🧹 Sanitized",
            vec![format!(
                "Removed hoisting characters from <@{}>.",
                member.get()
            )],
        )
        .kind(CardKind::Success),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::strip_hoisting;

    #[test]
    fn strips_hoisting_prefix() {
        assert_eq!(strip_hoisting("!John"), "John");
        assert_eq!(strip_hoisting("   ***Jane"), "Jane");
        assert_eq!(strip_hoisting("😀Jane"), "Jane");
        assert_eq!(strip_hoisting("!!! "), "");
        assert_eq!(strip_hoisting("123John"), "123John");
        assert_eq!(strip_hoisting("John"), "John");
    }
}
