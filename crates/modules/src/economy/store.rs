use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

use lumi_db::entities::{economy_account, economy_transaction, enums::EconomyTxnKind};
use lumi_kernel::{LumiError, Result};

use super::config::EconomyConfig;

const SLOT_SYMBOLS: [&str; 5] = ["🍒", "🍋", "🔔", "💎", "🍀"];

#[derive(Clone, Debug)]
pub struct SlotsOutcome {
    pub symbols: [String; 3],
    pub payout: i64,
    pub net: i64,
    pub new_wallet: i64,
}

#[derive(Clone, Debug)]
pub struct AccountBalance {
    pub wallet: i64,
    pub bank: i64,
    pub net_worth: i64,
}

#[derive(Clone, Debug)]
pub enum DailyOutcome {
    Claimed { amount: i64, wallet: i64 },
    OnCooldown { remaining_seconds: i64 },
    AtCap,
}

pub async fn get_or_create_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
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
            wallet: Set(cfg.starting_wallet.min(cfg.max_balance)),
            bank: Set(cfg.starting_bank.min(cfg.max_balance)),
            last_payday_at: Set(None),
        };
        Ok(new_acc.insert(db).await?)
    }
}

pub async fn get_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let acc = get_or_create_account(db, guild_id, user_id, cfg).await?;
    Ok(AccountBalance {
        wallet: acc.wallet,
        bank: acc.bank,
        net_worth: acc.wallet + acc.bank,
    })
}

pub async fn get_leaderboard(
    db: &DatabaseConnection,
    guild_id: u64,
    limit: u64,
    offset: u64,
) -> Result<Vec<economy_account::Model>> {
    use sea_orm::{ExprTrait, QueryOrder, QuerySelect};
    Ok(economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(guild_id.to_string()))
        .order_by_desc(sea_orm::sea_query::Expr::col(economy_account::Column::Wallet).add(sea_orm::sea_query::Expr::col(economy_account::Column::Bank)))
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?)
}

/// Count of economy accounts on the server (for leaderboard pagination).
pub async fn count_accounts(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    use sea_orm::PaginatorTrait;
    Ok(economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(guild_id.to_string()))
        .count(db)
        .await?)
}

pub async fn claim_daily(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<DailyOutcome> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;

    if let Some(last) = acc.last_payday_at {
        let elapsed = (Utc::now().fixed_offset() - last).num_seconds();
        let remaining = cfg.payday_cooldown_seconds - elapsed;
        if remaining > 0 {
            return Ok(DailyOutcome::OnCooldown {
                remaining_seconds: remaining,
            });
        }
    }

    if acc.wallet >= cfg.max_balance {
        return Ok(DailyOutcome::AtCap);
    }

    let amount = cfg.payday_amount.min(cfg.max_balance - acc.wallet);
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
    Ok(DailyOutcome::Claimed {
        amount,
        wallet: new_wallet,
    })
}

pub async fn admin_set_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    vault: &str,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;
    let amount = amount.min(cfg.max_balance);

    let (new_wallet, new_bank) = if vault == "bank" {
        (acc.wallet, amount)
    } else {
        (amount, acc.bank)
    };
    let delta = (new_wallet + new_bank) - (acc.wallet + acc.bank);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(EconomyTxnKind::AdminSet),
        amount: Set(delta),
        balance_after: Set(new_wallet + new_bank),
        reason: Set(Some(format!("Admin set {vault} to {amount}"))),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(AccountBalance {
        wallet: new_wallet,
        bank: new_bank,
        net_worth: new_wallet + new_bank,
    })
}

pub async fn admin_adjust_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    vault: &str,
    delta: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;

    let (new_wallet, new_bank) = if vault == "bank" {
        (acc.wallet, (acc.bank + delta).clamp(0, cfg.max_balance))
    } else {
        ((acc.wallet + delta).clamp(0, cfg.max_balance), acc.bank)
    };
    let actual_delta = (new_wallet + new_bank) - (acc.wallet + acc.bank);
    let kind = if delta >= 0 {
        EconomyTxnKind::AdminAdd
    } else {
        EconomyTxnKind::AdminRemove
    };

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    let log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(kind),
        amount: Set(actual_delta),
        balance_after: Set(new_wallet + new_bank),
        reason: Set(Some(format!("Admin adjust {vault} by {delta}"))),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    log.insert(&txn).await?;

    txn.commit().await?;
    Ok(AccountBalance {
        wallet: new_wallet,
        bank: new_bank,
        net_worth: new_wallet + new_bank,
    })
}

pub async fn play_slots(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    bid: i64,
    cfg: &EconomyConfig,
) -> Result<SlotsOutcome> {
    if bid < cfg.slot_min_bid || bid > cfg.slot_max_bid {
        return Err(LumiError::Other(format!(
            "Bid must be between {} and {}",
            cfg.slot_min_bid, cfg.slot_max_bid
        )));
    }

    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;

    if bid <= 0 || acc.wallet < bid {
        return Err(LumiError::Other("Insufficient wallet balance".to_string()));
    }

    let symbols = spin_symbols();
    let multiplier = if symbols[0] == symbols[1] && symbols[1] == symbols[2] {
        cfg.slot_payout_triple
    } else if symbols[0] == symbols[1] || symbols[1] == symbols[2] || symbols[0] == symbols[2] {
        cfg.slot_payout_pair
    } else {
        0
    };
    let payout = bid * multiplier;
    let after_bid = acc.wallet - bid;
    let new_wallet = (after_bid + payout).min(cfg.max_balance);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.update(&txn).await?;

    let reason = Some(format!("Slot machine [{}]", symbols.join(" ")));
    let bid_log = economy_transaction::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        kind: Set(EconomyTxnKind::SlotsBid),
        amount: Set(-bid),
        balance_after: Set(after_bid),
        reason: Set(reason.clone()),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    bid_log.insert(&txn).await?;

    if payout > 0 {
        let win_log = economy_transaction::ActiveModel {
            guild_id: Set(guild_id.to_string()),
            user_id: Set(user_id.to_string()),
            kind: Set(EconomyTxnKind::SlotsWin),
            amount: Set(payout),
            balance_after: Set(new_wallet),
            reason: Set(reason),
            created_at: Set(Utc::now().fixed_offset()),
            ..Default::default()
        };
        win_log.insert(&txn).await?;
    }

    txn.commit().await?;
    Ok(SlotsOutcome {
        symbols,
        payout,
        net: payout - bid,
        new_wallet,
    })
}

fn spin_symbols() -> [String; 3] {
    let mut hasher = RandomState::new().build_hasher();
    let mut out: [String; 3] = Default::default();
    for (i, slot) in out.iter_mut().enumerate() {
        hasher.write_u64(i as u64);
        *slot = SLOT_SYMBOLS[(hasher.finish() as usize) % SLOT_SYMBOLS.len()].to_string();
    }
    out
}

pub async fn transfer(
    db: &DatabaseConnection,
    guild_id: u64,
    sender_id: u64,
    recipient_id: u64,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<Option<i64>> {
    let txn = db.begin().await?;

    let sender = get_or_create_account(&txn, guild_id, sender_id, cfg).await?;
    if sender.wallet < amount {
        return Ok(None);
    }

    let recipient = get_or_create_account(&txn, guild_id, recipient_id, cfg).await?;

    let tax = cfg.transfer_tax(amount);
    let credited = (amount - tax).min((cfg.max_balance - recipient.wallet).max(0));

    let sender_new = sender.wallet - amount;
    let recipient_new = recipient.wallet + credited;

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
    cfg: &EconomyConfig,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;

    if acc.wallet < amount || amount <= 0 {
        return Ok(None);
    }

    let new_wallet = acc.wallet - amount;
    let new_bank = (acc.bank + amount).min(cfg.max_balance);

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
    cfg: &EconomyConfig,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = get_or_create_account(&txn, guild_id, user_id, cfg).await?;

    if acc.bank < amount || amount <= 0 {
        return Ok(None);
    }

    let new_wallet = (acc.wallet + amount).min(cfg.max_balance);
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
