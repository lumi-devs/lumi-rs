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
#[poise::command(slash_command, prefix_command, guild_only)]
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
    subcommands("deposit_cmd", "withdraw_cmd")
)]
pub async fn bank(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "🏦 Server Bank",
        "Use `/bank deposit <amount>` or `/bank withdraw <amount>` to manage funds.",
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

pub fn commands() -> Vec<Command> {
    vec![balance(), daily(), pay(), bank()]
}
