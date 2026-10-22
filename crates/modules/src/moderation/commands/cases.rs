//! Case lookup, edit and moderator-note commands.

use serenity::all::UserId;

use lumi_kernel::bot::{Context, Error};
use lumi_ui::{Card, CardKind};

use crate::moderation::store;
use crate::moderation::views::{action_from_str, case_card, notes_list_card};

use super::{guild_id, send_card};

/// View, modify, or delete moderation cases.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    subcommands("cases_view", "cases_modify", "cases_delete")
)]
pub async fn cases(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    send_card(
        ctx,
        Card::lines(
            "📋 Moderation Cases",
            vec![
                "Use the subcommands to interact with cases:".to_string(),
                "• `/cases view [member] [number] [action]` - View case details or list member cases".to_string(),
                "• `/cases modify <number> <reason>` - Update case reason".to_string(),
                "• `/cases delete <number>` - Delete a case".to_string(),
            ],
        ),
    )
    .await
}

async fn show_case(ctx: Context<'_>, guild_id: u64, number: i32) -> Result<(), Error> {
    match store::get_case(&ctx.data().db, guild_id, number).await? {
        Some(case) => {
            ctx.send(case_card(&case).reply()).await?;
        }
        None => {
            send_card(
                ctx,
                Card::lines(
                    "🔍 Case Not Found",
                    vec![format!("Case `#{number}` does not exist in this server.")],
                )
                .kind(CardKind::Error),
            )
            .await?;
        }
    }
    Ok(())
}

/// View a specific moderation case or list cases for a user.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "view",
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn cases_view(
    ctx: Context<'_>,
    #[description = "Specific case number"] number: Option<i32>,
    #[description = "Filter by member"] member: Option<UserId>,
    #[description = "Filter by action (warn, kick, ban, etc.)"] action: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = guild_id(ctx)?.get();

    if let Some(number) = number {
        return show_case(ctx, guild_id, number).await;
    }

    let action_filter = match action.as_deref() {
        Some(raw) => match action_from_str(raw) {
            Some(action) => Some(action),
            None => {
                return send_card(
                    ctx,
                    Card::lines(
                        "❌ Unknown Action",
                        vec![format!(
                            "`{raw}` is not a known action. Use warn, kick, ban, mute, softban, quarantine, unban, unmute."
                        )],
                    )
                    .kind(CardKind::Error),
                )
                .await;
            }
        },
        None => None,
    };

    let cases = store::list_cases(
        &ctx.data().db,
        guild_id,
        member.map(UserId::get),
        action_filter,
        50,
    )
    .await?;

    if cases.is_empty() {
        return send_card(
            ctx,
            Card::lines(
                "📋 No Cases Found",
                vec!["No moderation cases match your query.".to_string()],
            ),
        )
        .await;
    }

    let lines = cases
        .iter()
        .take(15)
        .map(|case| {
            let action = crate::moderation::views::action_label(case.action).to_uppercase();
            let reason = case.reason.as_deref().unwrap_or("No reason provided");
            format!(
                "**#{}** `{action}` for <@{}>: {reason} (<t:{}:R>)",
                case.case_number,
                case.user_id,
                case.created_at.timestamp()
            )
        })
        .collect::<Vec<_>>();

    let options = lumi_ui::CardOptions {
        footer: Some(format!(
            "Showing up to 15 of {} matching case(s)",
            cases.len()
        )),
        ..lumi_ui::CardOptions::default()
    };
    let title = member.map_or_else(
        || "📋 Server Moderation Cases".to_string(),
        |user| format!("📋 Cases for <@{}>", user.get()),
    );
    ctx.send(
        Card::lines(title, lines)
            .options(options)
            .into_reply()
            .reply(),
    )
    .await?;
    Ok(())
}

async fn update_reason(
    ctx: Context<'_>,
    guild_id: u64,
    number: i32,
    reason: &str,
) -> Result<(), Error> {
    if store::update_case_reason(&ctx.data().db, guild_id, number, reason).await? {
        send_card(
            ctx,
            Card::lines(
                "✅ Reason Updated",
                vec![format!(
                    "Updated reason for Case `#{number}` to: **{reason}**"
                )],
            )
            .kind(CardKind::Success),
        )
        .await
    } else {
        send_card(
            ctx,
            Card::lines(
                "❌ Case Not Found",
                vec![format!("Case `#{number}` does not exist.")],
            )
            .kind(CardKind::Error),
        )
        .await
    }
}

/// Update the reason of an existing moderation case.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "modify",
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn cases_modify(
    ctx: Context<'_>,
    #[description = "Case number"] number: i32,
    #[description = "New reason"] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = guild_id(ctx)?.get();
    update_reason(ctx, guild_id, number, &reason).await
}

/// Delete an existing moderation case.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "delete",
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn cases_delete(
    ctx: Context<'_>,
    #[description = "Case number to delete"] number: i32,
) -> Result<(), Error> {
    let body = format!("Are you sure you want to permanently delete Case `#{number}`?");
    if !lumi_ui::confirm_poise(
        ctx,
        "⚠️ Confirm Case Deletion",
        &body,
        "Delete Case",
        std::time::Duration::from_secs(30),
    )
    .await?
    {
        return send_card(
            ctx,
            Card::lines(
                "Action Cancelled",
                vec!["Case deletion was cancelled.".to_string()],
            ),
        )
        .await;
    }

    ctx.defer().await?;
    let guild_id = guild_id(ctx)?.get();
    if store::delete_case(&ctx.data().db, guild_id, number)
        .await?
        .is_some()
    {
        send_card(
            ctx,
            Card::lines(
                "🗑️ Case Deleted",
                vec![format!(
                    "Moderation Case `#{number}` has been permanently deleted."
                )],
            )
            .kind(CardKind::Success),
        )
        .await
    } else {
        send_card(
            ctx,
            Card::lines(
                "❌ Case Not Found",
                vec![format!("Case `#{number}` does not exist.")],
            )
            .kind(CardKind::Error),
        )
        .await
    }
}

/// Inspect a specific moderation case.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn case(
    ctx: Context<'_>,
    #[description = "Case number to lookup"] number: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    show_case(ctx, guild_id(ctx)?.get(), number).await
}

/// Update the reason of an existing moderation case.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn reason(
    ctx: Context<'_>,
    #[description = "Case number"] number: i32,
    #[description = "New reason"] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    update_reason(ctx, guild_id(ctx)?.get(), number, &reason).await
}

/// View or add moderator notes for a member.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS"
)]
pub async fn note(
    ctx: Context<'_>,
    #[description = "Target user"] user: UserId,
    #[description = "Note text to append (leave empty to view notes)"] text: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = guild_id(ctx)?.get();

    if let Some(content) = text {
        store::add_note(
            &ctx.data().db,
            guild_id,
            user.get(),
            ctx.author().id.get(),
            &content,
        )
        .await?;
        send_card(
            ctx,
            Card::lines(
                "📝 Note Added",
                vec![format!("Added note to <@{}>: **{content}**", user.get())],
            ),
        )
        .await
    } else {
        let notes = store::list_notes(&ctx.data().db, guild_id, user.get()).await?;
        ctx.send(notes_list_card(user.get(), &notes).reply())
            .await?;
        Ok(())
    }
}
