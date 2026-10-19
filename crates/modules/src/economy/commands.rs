use poise::serenity_prelude as serenity;
use serenity::all::UserId;

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};

use super::store;

/// View your wallet and bank balance, or check another member's account.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn balance(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let target_id = user.unwrap_or_else(|| ctx.author().id).get();

    let bal = store::get_balance(&ctx.data().db, guild_id, target_id).await?;

    let lines = vec![
        format!("**Wallet**: `{}` coins", bal.wallet),
        format!("**Bank**: `{}` coins", bal.bank),
        format!("**Net Worth**: `{}` coins", bal.net_worth),
    ];

    let card = Card::lines(format!("💰 Balance — <@{target_id}>"), lines)
        .kind(CardKind::Gold);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Claim your daily coin allowance.
#[poise::command(slash_command, prefix_command, guild_only, aliases("payday"))]
pub async fn daily(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();
    let reward = 250;

    let new_balance = store::claim_daily(&ctx.data().db, guild_id, user_id, reward).await?;

    let card = Card::new(
        "✨ Daily Reward Claimed",
        format!("You received **+{reward}** coins! New wallet balance: `{new_balance}` coins."),
    )
    .kind(CardKind::Success);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Pay coins to another server member.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pay(
    ctx: Context<'_>,
    #[description = "Recipient"] user: UserId,
    #[description = "Amount to send"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
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

    let result = store::transfer(&ctx.data().db, guild_id, sender_id, recipient_id, amount).await?;

    match result {
        Some(remaining) => {
            let card = Card::new(
                "💸 Transfer Successful",
                format!("Transferred **{amount}** coins to <@{recipient_id}>. Remaining wallet: `{remaining}` coins."),
            )
            .kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new("❌ Insufficient Funds", "You do not have enough coins in your wallet for this transfer.")
                .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
    }

    Ok(())
}

/// Manage bank transactions: deposit and withdraw coins.
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

/// Deposit coins from your wallet into the bank.
#[poise::command(slash_command, prefix_command, rename = "deposit", guild_only)]
pub async fn deposit_cmd(
    ctx: Context<'_>,
    #[description = "Amount to deposit"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    if amount <= 0 {
        let card = Card::new("❌ Invalid Amount", "Deposit amount must be greater than zero.").kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    match store::deposit(&ctx.data().db, guild_id, user_id, amount).await? {
        Some((wallet, bank)) => {
            let card = Card::new(
                "🏦 Deposit Successful",
                format!("Deposited **{amount}** coins into your bank.\nWallet: `{wallet}` coins · Bank: `{bank}` coins."),
            ).kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new("❌ Insufficient Funds", "You do not have enough coins in your wallet.").kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

/// Withdraw coins from your bank into your wallet.
#[poise::command(slash_command, prefix_command, rename = "withdraw", guild_only)]
pub async fn withdraw_cmd(
    ctx: Context<'_>,
    #[description = "Amount to withdraw"] amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    if amount <= 0 {
        let card = Card::new("❌ Invalid Amount", "Withdrawal amount must be greater than zero.").kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    match store::withdraw(&ctx.data().db, guild_id, user_id, amount).await? {
        Some((wallet, bank)) => {
            let card = Card::new(
                "🏦 Withdrawal Successful",
                format!("Withdrew **{amount}** coins from your bank.\nWallet: `{wallet}` coins · Bank: `{bank}` coins."),
            ).kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        None => {
            let card = Card::new("❌ Insufficient Funds", "You do not have enough coins in your bank.").kind(CardKind::Error);
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

fn parse_vault(vault: &Option<String>) -> Option<&'static str> {
    match vault.as_deref() {
        None | Some("wallet") => Some("wallet"),
        Some("bank") => Some("bank"),
        _ => None,
    }
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
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
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

    let Some(vault) = parse_vault(&vault) else {
        let card = Card::new("❌ Invalid Vault", "Vault must be either `wallet` or `bank`.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    let reason = reason.unwrap_or_else(|| "no reason given".to_string());

    let balance = match op {
        AdminOp::Set => store::admin_set_balance(&ctx.data().db, guild_id, target_id, vault, amount).await?,
        AdminOp::Add => store::admin_adjust_balance(&ctx.data().db, guild_id, target_id, vault, amount).await?,
        AdminOp::Remove => store::admin_adjust_balance(&ctx.data().db, guild_id, target_id, vault, -amount).await?,
    };

    let (title, verb) = match op {
        AdminOp::Set => ("🏦 Balance Set", "set"),
        AdminOp::Add => ("🏦 Balance Added", "added"),
        AdminOp::Remove => ("🏦 Balance Removed", "removed"),
    };

    let card = Card::lines(
        title,
        vec![
            format!("{verb} **{amount}** coins in <@{target_id}>'s {vault}."),
            format!("Reason: {reason}"),
            format!("**New Net Worth**: `{}` coins", balance.net_worth),
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

/// Grant coins to a member's wallet or bank (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "add",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn add_cmd(
    ctx: Context<'_>,
    #[description = "Who receives the coins"] user: UserId,
    #[description = "How much to add"] amount: i64,
    #[description = "Which balance to credit (wallet or bank)"] vault: Option<String>,
    #[description = "Why the coins are granted"] reason: Option<String>,
) -> Result<(), Error> {
    admin_adjust(ctx, AdminOp::Add, user, amount, vault, reason).await
}

/// Take coins from a member's wallet or bank (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "remove",
    default_member_permissions = "MANAGE_GUILD"
)]
pub async fn remove_cmd(
    ctx: Context<'_>,
    #[description = "Who loses the coins"] user: UserId,
    #[description = "How much to remove"] amount: i64,
    #[description = "Which balance to debit (wallet or bank)"] vault: Option<String>,
    #[description = "Why the coins are removed"] reason: Option<String>,
) -> Result<(), Error> {
    admin_adjust(ctx, AdminOp::Remove, user, amount, vault, reason).await
}

/// Bet wallet coins on the slot machine.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn slots(
    ctx: Context<'_>,
    #[description = "How much to bet"] bid: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    if bid <= 0 {
        let card = Card::new("❌ Invalid Amount", "Bid must be greater than zero.")
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    match store::play_slots(&ctx.data().db, guild_id, user_id, bid).await {
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
                    format!("Bid: `{bid}` coins · Won: `{}` coins", outcome.payout),
                    format!("**Wallet**: `{}` coins", outcome.new_wallet),
                ],
            )
            .kind(kind);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(err) if err.to_string().contains("Insufficient") => {
            let card = Card::new(
                "❌ Insufficient Funds",
                "You do not have enough coins in your wallet for that bet.",
            )
            .kind(CardKind::Error);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(err) => return Err(err.into()),
    }

    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![balance(), daily(), pay(), bank(), slots()]
}
