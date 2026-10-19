use poise::serenity_prelude as serenity;
use serenity::all::{
    ComponentInteraction, CreateAllowedMentions, CreateInteractionResponse,
    CreateInteractionResponseMessage, MessageFlags, UserId,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::panels::pagination_row;
use lumi_ui::{Card, CardKind, CardReply};

use crate::shared::{
    claim_cooldown, cooldown_key, format_seconds, page_bounds, require_guild, secs_ceil,
};

use super::config::EconomyConfig;
use super::store::{self, DailyOutcome, SlotsError, Vault};

const LEADERBOARD_PER_PAGE: u64 = 10;

/// View your wallet and bank balance, or check another member's account.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn balance(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let target_id = user.unwrap_or_else(|| ctx.author().id);

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    let bal = store::get_balance(&ctx.data().db, guild_id, target_id.get(), &cfg).await?;
    let target = target_id.to_user(ctx.http()).await?;

    let lines = vec![
        format!("Wallet: **{}**", cfg.format_amount(bal.wallet)),
        format!("{}: **{}**", cfg.bank_name, cfg.format_amount(bal.bank)),
        format!("Total: **{}**", cfg.format_amount(bal.net_worth)),
    ];

    let card = Card::lines(format!("💰 {}", target.name), lines).kind(CardKind::Gold);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Claim your daily allowance.
#[poise::command(slash_command, prefix_command, guild_only, aliases("payday"))]
pub async fn daily(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    let card = match store::claim_daily(&ctx.data().db, guild_id, user_id, &cfg).await? {
        DailyOutcome::Claimed { amount, wallet } => Card::new(
            "✨ Daily Reward Claimed",
            format!(
                "You received **+{}**! New wallet balance: **{}**.",
                cfg.format_amount(amount),
                cfg.format_amount(wallet)
            ),
        )
        .kind(CardKind::Success),
        DailyOutcome::OnCooldown { remaining_seconds } => Card::new(
            "⏳ Already Claimed",
            format!(
                "You can claim again in **{}**.",
                format_seconds(remaining_seconds)
            ),
        )
        .kind(CardKind::Warning),
        DailyOutcome::AtCap => Card::new(
            "🏦 Balance Capped",
            format!(
                "Your wallet is already at the **{}** cap.",
                cfg.format_amount(cfg.max_balance)
            ),
        )
        .kind(CardKind::Warning),
    };

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Pay money to another server member.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pay(
    ctx: Context<'_>,
    #[description = "Recipient"] user: UserId,
    #[description = "Amount to send"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let sender_id = ctx.author().id.get();
    let recipient_id = user.get();

    if amount <= 0 {
        let card = Card::new("❌ Invalid Amount", "Payment amount must be greater than zero.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    if sender_id == recipient_id {
        let card = Card::new("❌ Self Payment", "You cannot send money to yourself.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    let result =
        store::transfer(&ctx.data().db, guild_id, sender_id, recipient_id, amount, &cfg).await?;

    match result {
        Some(remaining) => {
            let tax = cfg.transfer_tax(amount);
            let tax_line = if tax > 0 {
                format!("\nTax withheld: **{}**.", cfg.format_amount(tax))
            } else {
                String::new()
            };
            let card = Card::new(
                "💸 Transfer Successful",
                format!(
                    "Transferred **{}** to <@{recipient_id}>.{tax_line}\nYour wallet: **{}**.",
                    cfg.format_amount(amount),
                    cfg.format_amount(remaining)
                ),
            )
            .kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new(
                "❌ Insufficient Funds",
                "You do not have enough funds in your wallet for this transfer.",
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
    }

    Ok(())
}

/// Manage bank transactions: deposit and withdraw funds.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    subcommands("deposit_cmd", "withdraw_cmd", "set_cmd", "add_cmd", "remove_cmd")
)]
pub async fn bank(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "🏦 Server Bank",
        "Use `/bank deposit <amount>` or `/bank withdraw <amount>` to manage funds. Staff can use `/bank set`, `/bank add`, or `/bank remove` to adjust balances.",
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Deposit funds from your wallet into the bank.
#[poise::command(slash_command, prefix_command, rename = "deposit", guild_only)]
pub async fn deposit_cmd(
    ctx: Context<'_>,
    #[description = "Amount to deposit"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    if amount <= 0 {
        let card = Card::new("❌ Invalid Amount", "Deposit amount must be greater than zero.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    match store::deposit(&ctx.data().db, guild_id, user_id, amount, &cfg).await? {
        Some((wallet, bank)) => {
            let card = Card::new(
                "🏦 Deposit Successful",
                format!(
                    "Moved **{}** into your {}.\nWallet: **{}** · {}: **{}**",
                    cfg.format_amount(amount),
                    cfg.bank_name,
                    cfg.format_amount(wallet),
                    cfg.bank_name,
                    cfg.format_amount(bank)
                ),
            )
            .kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new(
                "❌ Deposit Failed",
                "You do not have enough wallet funds, or the bank is already at its cap.",
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

/// Withdraw funds from your bank into your wallet.
#[poise::command(slash_command, prefix_command, rename = "withdraw", guild_only)]
pub async fn withdraw_cmd(
    ctx: Context<'_>,
    #[description = "Amount to withdraw"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    if amount <= 0 {
        let card = Card::new(
            "❌ Invalid Amount",
            "Withdrawal amount must be greater than zero.",
        )
        .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    match store::withdraw(&ctx.data().db, guild_id, user_id, amount, &cfg).await? {
        Some((wallet, bank)) => {
            let card = Card::new(
                "🏦 Withdrawal Successful",
                format!(
                    "Moved **{}** into your wallet.\nWallet: **{}** · {}: **{}**",
                    cfg.format_amount(amount),
                    cfg.format_amount(wallet),
                    cfg.bank_name,
                    cfg.format_amount(bank)
                ),
            )
            .kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new(
                "❌ Withdrawal Failed",
                "You do not have enough bank funds, or your wallet is already at its cap.",
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

enum AdminOp {
    Set,
    Add,
    Remove,
}

async fn admin_adjust(
    ctx: Context<'_>,
    op: AdminOp,
    user: UserId,
    amount: i64,
    vault: Option<String>,
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let target_id = user.get();

    let invalid_amount = match op {
        AdminOp::Set => amount < 0,
        AdminOp::Add | AdminOp::Remove => amount < 1,
    };
    if invalid_amount {
        let message = match op {
            AdminOp::Set => "Amount must be zero or greater.",
            AdminOp::Add | AdminOp::Remove => "Amount must be greater than zero.",
        };
        let card = Card::new("❌ Invalid Amount", message).kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let Some(vault) = Vault::parse(vault.as_deref()) else {
        let card = Card::new("❌ Invalid Vault", "Vault must be either `wallet` or `bank`.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    let reason = reason.unwrap_or_else(|| "no reason given".to_string());

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;
    let balance = match op {
        AdminOp::Set => {
            store::admin_set_balance(&ctx.data().db, guild_id, target_id, vault, amount, &cfg)
                .await?
        }
        AdminOp::Add => {
            store::admin_adjust_balance(&ctx.data().db, guild_id, target_id, vault, amount, &cfg)
                .await?
        }
        AdminOp::Remove => {
            store::admin_adjust_balance(&ctx.data().db, guild_id, target_id, vault, -amount, &cfg)
                .await?
        }
    };

    let (title, verb) = match op {
        AdminOp::Set => ("🏦 Balance Set", "set"),
        AdminOp::Add => ("🏦 Balance Added", "added"),
        AdminOp::Remove => ("🏦 Balance Removed", "removed"),
    };

    let card = Card::lines(
        title,
        vec![
            format!(
                "{verb} **{}** in <@{target_id}>'s {}.",
                cfg.format_amount(amount),
                vault.label()
            ),
            format!("Reason: {reason}"),
            format!("**New total**: **{}**", cfg.format_amount(balance.net_worth)),
        ],
    )
    .kind(CardKind::Success);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Set a member's wallet or bank balance (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "set",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn set_cmd(
    ctx: Context<'_>,
    #[description = "Whose balance to set"] user: UserId,
    #[description = "The new balance"] amount: i64,
    #[description = "Which balance to set (wallet or bank)"] vault: Option<String>,
    #[description = "Why the balance is changed"] reason: Option<String>,
) -> Result<(), Error> {
    admin_adjust(ctx, AdminOp::Set, user, amount, vault, reason).await
}

/// Grant funds to a member's wallet or bank (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "add",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn add_cmd(
    ctx: Context<'_>,
    #[description = "Who receives the funds"] user: UserId,
    #[description = "How much to add"] amount: i64,
    #[description = "Which balance to credit (wallet or bank)"] vault: Option<String>,
    #[description = "Why the funds are granted"] reason: Option<String>,
) -> Result<(), Error> {
    admin_adjust(ctx, AdminOp::Add, user, amount, vault, reason).await
}

/// Take funds from a member's wallet or bank (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "remove",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn remove_cmd(
    ctx: Context<'_>,
    #[description = "Who loses the funds"] user: UserId,
    #[description = "How much to remove"] amount: i64,
    #[description = "Which balance to debit (wallet or bank)"] vault: Option<String>,
    #[description = "Why the funds are removed"] reason: Option<String>,
) -> Result<(), Error> {
    admin_adjust(ctx, AdminOp::Remove, user, amount, vault, reason).await
}

/// Bet wallet funds on the slot machine.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn slots(
    ctx: Context<'_>,
    #[description = "How much to bet"] bid: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    let cfg = EconomyConfig::load(&ctx.data().db, guild_id).await?;

    if bid < cfg.slot_min_bid || bid > cfg.slot_max_bid {
        let card = Card::new(
            "❌ Invalid Bid",
            format!(
                "Bid must be between **{}** and **{}**.",
                cfg.format_amount(cfg.slot_min_bid),
                cfg.format_amount(cfg.slot_max_bid)
            ),
        )
        .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let key = cooldown_key("econ:slots", guild_id, user_id);
    if let Some(remaining) =
        claim_cooldown(&ctx.data().cache, &key, cfg.cooldown_ms(cfg.slot_cooldown_seconds)).await?
    {
        let card = Card::new(
            "⏳ Slow Down",
            format!(
                "The reels are still spinning. Try again in **{}**.",
                format_seconds(secs_ceil(remaining))
            ),
        )
        .kind(CardKind::Warning);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    match store::play_slots(&ctx.data().db, guild_id, user_id, bid, &cfg).await {
        Ok(outcome) => {
            let (title, kind) = if outcome.net > 0 {
                ("🎰 Winner!", CardKind::Success)
            } else {
                ("🎰 Nothing!", CardKind::Info)
            };
            let card = Card::lines(
                title,
                vec![
                    outcome.symbols.join("  "),
                    format!(
                        "Bid: **{}** · Won: **{}**",
                        cfg.format_amount(bid),
                        cfg.format_amount(outcome.payout)
                    ),
                    format!("**Wallet**: **{}**", cfg.format_amount(outcome.new_wallet)),
                ],
            )
            .kind(kind);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(SlotsError::InsufficientFunds) => {
            let card = Card::new(
                "❌ Insufficient Funds",
                "You do not have enough funds in your wallet for that bet.",
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(SlotsError::InvalidBid { min, max }) => {
            let card = Card::new(
                "❌ Invalid Bid",
                format!(
                    "Bid must be between **{}** and **{}**.",
                    cfg.format_amount(min),
                    cfg.format_amount(max)
                ),
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(SlotsError::Other(err)) => return Err(err.into()),
    }

    Ok(())
}

/// Show the richest members on this server.
#[poise::command(slash_command, prefix_command, guild_only, aliases("lb", "top"))]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let guild_name = ctx
        .guild()
        .map_or_else(|| "this server".to_string(), |g| g.name.to_string());
    let caller_id = ctx.author().id.get();

    let reply = leaderboard_reply(&ctx.data().db, guild_id, &guild_name, 0, caller_id).await?;
    ctx.send(reply.reply()).await?;
    Ok(())
}

/// Re-render the economy leaderboard at a requested page (pager buttons).
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let mut parts = interaction.data.custom_id.split(':');
    if parts.next() != Some("econ") || parts.next() != Some("lb") {
        return Ok(());
    }
    let Some(page) = parts.next().and_then(|p| p.parse::<usize>().ok()) else {
        return Ok(());
    };
    let Some(guild_id) = interaction.guild_id else {
        return Ok(());
    };
    let guild_name = ctx
        .cache
        .guild(guild_id)
        .map_or_else(|| "this server".to_string(), |g| g.name.to_string());
    let caller_id = interaction.user.id.get();

    let reply = leaderboard_reply(&services.db, guild_id.get(), &guild_name, page, caller_id).await?;
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(reply.components)
                    .allowed_mentions(CreateAllowedMentions::new())
                    .flags(MessageFlags::IS_COMPONENTS_V2),
            ),
        )
        .await?;
    Ok(())
}

async fn leaderboard_reply(
    db: &sea_orm::DatabaseConnection,
    guild_id: u64,
    guild_name: &str,
    page: usize,
    caller_id: u64,
) -> Result<CardReply, Error> {
    let total = store::count_accounts(db, guild_id).await?;
    if total == 0 {
        let card = Card::lines(
            "🏆 Leaderboard",
            vec![
                "Nobody has an economy account on this server yet.".to_string(),
                "Claim a payday or ask staff to grant currency.".to_string(),
            ],
        )
        .kind(CardKind::Gold);
        return Ok(card.into_reply());
    }

    let (page, total_pages, offset) = page_bounds(total, LEADERBOARD_PER_PAGE, page);

    let cfg = EconomyConfig::load(db, guild_id).await?;
    let entries = store::get_leaderboard(db, guild_id, LEADERBOARD_PER_PAGE, offset).await?;

    let mut lines = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let rank = usize::try_from(offset).unwrap_or(usize::MAX) + idx + 1;
        let total_amount = entry.wallet.saturating_add(entry.bank);
        let marker = if entry.user_id.get() == caller_id {
            " **← you**"
        } else {
            ""
        };
        lines.push(format!(
            "**#{rank}** <@{}> - {}{marker}",
            entry.user_id,
            cfg.format_amount(total_amount)
        ));
    }

    let mut opts = lumi_ui::CardOptions::default();
    opts.footer = Some(format!("Page {} of {total_pages}", page + 1));
    opts.action_rows = pagination_row("econ:lb", page, total_pages)
        .into_iter()
        .collect();

    Ok(Card::lines(format!("🏆 Richest in {guild_name}"), lines)
        .kind(CardKind::Gold)
        .options(opts)
        .into_reply())
}

pub fn commands() -> Vec<Command> {
    vec![balance(), daily(), pay(), bank(), slots(), leaderboard()]
}
