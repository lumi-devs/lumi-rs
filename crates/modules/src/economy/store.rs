use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

use lumi_db::entities::{economy_account, economy_transaction, enums::EconomyTxnKind};
use lumi_kernel::Result;

#[derive(Clone, Debug)]
pub struct AccountBalance {
    pub wallet: i64,
    pub bank: i64,
    pub net_worth: i64,
}

pub async fn get_or_create_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<economy_account::Model> {
    let existing = economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(guild_id.to_string()))
        .filter(economy_account::Column::UserId.eq(user_id.to_string()))
        .one(db)
        .await?;

    if let Some(acc) = existing {
        Ok(acc)
    } else {
        let new_acc = economy_account::ActiveModel {
            guild_id: Set(guild_id.to_string()),
            user_id: Set(user_id.to_string()),
            wallet: Set(100),
            bank: Set(0),
            last_payday_at: Set(None),
        };
        Ok(new_acc.insert(db).await?)
    }
}

pub async fn get_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<AccountBalance> {
    let acc = get_or_create_account(db, guild_id, user_id).await?;
    Ok(AccountBalance {
        wallet: acc.wallet,
        bank: acc.bank,
        net_worth: acc.wallet + acc.bank,
    })
}

pub async fn claim_daily(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
) -> Result<i64> {
    let txn = db.begin().await?;

    let acc = get_or_create_account(&txn, guild_id, user_id).await?;
    let new_wallet = acc.wallet + amount;

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.last_payday_at = Set(Some(Utc::now().fixed_offset()));
    active.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(EconomyTxnKind::Payday),
        amount: Set(amount),
        balance_after: Set(new_wallet),
        reason: Set(Some("Daily allowance".to_string())),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(new_wallet)
}

pub async fn transfer(
    db: &DatabaseConnection,
    guild_id: u64,
    sender_id: u64,
    recipient_id: u64,
    amount: i64,
) -> Result<Option<i64>> {
    let txn = db.begin().await?;

    let sender = get_or_create_account(&txn, guild_id, sender_id).await?;
    if sender.wallet < amount {
        return Ok(None);
    }

    let recipient = get_or_create_account(&txn, guild_id, recipient_id).await?;

    let sender_new = sender.wallet - amount;
    let recipient_new = recipient.wallet + amount;

    let mut sender_act: economy_account::ActiveModel = sender.into();
    sender_act.wallet = Set(sender_new);
    sender_act.update(&txn).await?;

    let mut recipient_act: economy_account::ActiveModel = recipient.into();
    recipient_act.wallet = Set(recipient_new);
    recipient_act.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(sender_id.to_string()),
        kind: Set(EconomyTxnKind::TransferOut),
        amount: Set(-amount),
        balance_after: Set(sender_new),
        reason: Set(Some(format!("Transfer to {recipient_id}"))),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(Some(sender_new))
}

pub async fn deposit(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id).await?;

    if acc.wallet < amount || amount <= 0 {
        return Ok(None);
    }

    let new_wallet = acc.wallet - amount;
    let new_bank = acc.bank + amount;

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(EconomyTxnKind::TransferOut),
        amount: Set(-amount),
        balance_after: Set(new_wallet),
        reason: Set(Some("Bank deposit".to_string())),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(Some((new_wallet, new_bank)))
}

pub async fn withdraw(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id).await?;

    if acc.bank < amount || amount <= 0 {
        return Ok(None);
    }

    let new_wallet = acc.wallet + amount;
    let new_bank = acc.bank - amount;

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(EconomyTxnKind::TransferIn),
        amount: Set(amount),
        balance_after: Set(new_wallet),
        reason: Set(Some("Bank withdrawal".to_string())),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(Some((new_wallet, new_bank)))
}
