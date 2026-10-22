use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::OnConflict,
};

use lumi_db::entities::{economy_account, economy_transaction, enums::EconomyTxnKind};
use lumi_kernel::{LumiError, Result};

use super::config::EconomyConfig;

const MAX_ATTEMPTS: usize = 3;
const INITIAL_BACKOFF_MS: u64 = 25;

fn is_conflict_db_err(err: &sea_orm::DbErr) -> bool {
    if let sea_orm::DbErr::Exec(sea_orm::RuntimeErr::SqlxError(e))
    | sea_orm::DbErr::Query(sea_orm::RuntimeErr::SqlxError(e))
    | sea_orm::DbErr::Conn(sea_orm::RuntimeErr::SqlxError(e)) = err
        && let sea_orm::SqlxError::Database(db_err) = &**e
        && let Some(code) = db_err.code()
        && (code.as_ref() == "40001" || code.as_ref() == "40P01")
    {
        return true;
    }
    let msg = err.to_string();
    msg.contains("40001") || msg.contains("40P01")
}

fn is_conflict_error(err: &LumiError) -> bool {
    if let LumiError::Db(db_err) = err {
        is_conflict_db_err(db_err)
    } else {
        let msg = err.to_string();
        msg.contains("40001") || msg.contains("40P01")
    }
}

fn is_slots_conflict(err: &SlotsError) -> bool {
    match err {
        SlotsError::Other(lumi_err) => is_conflict_error(lumi_err),
        _ => false,
    }
}

macro_rules! run_with_retry {
    ($op:expr) => {{
        let mut attempt = 0;
        loop {
            attempt += 1;
            match $op.await {
                Ok(val) => break Ok(val),
                Err(err) if attempt < MAX_ATTEMPTS && is_conflict_error(&err) => {
                    let backoff = INITIAL_BACKOFF_MS * (1 << (attempt - 1));
                    tracing::warn!(
                        attempt,
                        backoff_ms = backoff,
                        error = %err,
                        "retrying transaction after conflict (40001/40P01)"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
                }
                Err(err) => break Err(err),
            }
        }
    }};
}

const SLOT_SYMBOLS: [&str; 5] = ["🍒", "🍋", "🔔", "💎", "🍀"];

/// Which balance an admin operation targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vault {
    Wallet,
    Bank,
}

impl Vault {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value {
            None | Some("wallet") => Some(Self::Wallet),
            Some("bank") => Some(Self::Bank),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Wallet => "wallet",
            Self::Bank => "bank",
        }
    }
}

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

const fn balance(acc: &economy_account::Model) -> AccountBalance {
    AccountBalance {
        wallet: acc.wallet,
        bank: acc.bank,
        net_worth: acc.wallet.saturating_add(acc.bank),
    }
}

#[derive(Clone, Debug)]
pub enum DailyOutcome {
    Claimed { amount: i64, wallet: i64 },
    OnCooldown { remaining_seconds: i64 },
    AtCap,
}

#[derive(Debug, thiserror::Error)]
pub enum SlotsError {
    #[error("bid must be between {min} and {max}")]
    InvalidBid { min: i64, max: i64 },
    #[error("insufficient wallet balance")]
    InsufficientFunds,
    #[error(transparent)]
    Other(#[from] LumiError),
}

impl From<sea_orm::DbErr> for SlotsError {
    fn from(err: sea_orm::DbErr) -> Self {
        Self::Other(LumiError::from(err))
    }
}

async fn find_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<economy_account::Model>> {
    Ok(economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(economy_account::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .one(db)
        .await?)
}

/// Find or create the account without taking a row lock. Safe under concurrency:
/// the insert is `ON CONFLICT DO NOTHING` followed by a read.
pub async fn get_or_create_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<economy_account::Model> {
    if let Some(acc) = find_account(db, guild_id, user_id).await? {
        return Ok(acc);
    }

    let row = economy_account::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        wallet: Set(cfg.starting_wallet.min(cfg.max_balance).max(0)),
        bank: Set(cfg.starting_bank.min(cfg.max_balance).max(0)),
        last_payday_at: Set(None),
    };
    economy_account::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([
                economy_account::Column::GuildId,
                economy_account::Column::UserId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(db)
        .await?;

    find_account(db, guild_id, user_id)
        .await?
        .ok_or_else(|| LumiError::Other("economy account insert was lost".to_string()))
}

/// Account row locked with `SELECT … FOR UPDATE`; mutation paths must use this
/// so concurrent transfers/claims cannot read stale balances.
async fn locked_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<economy_account::Model> {
    get_or_create_account(db, guild_id, user_id, cfg).await?;
    economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(economy_account::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or_else(|| LumiError::Other("economy account disappeared".to_string()))
}

async fn log_transaction<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
    kind: EconomyTxnKind,
    amount: i64,
    balance_after: i64,
    reason: impl Into<String>,
) -> Result<()> {
    economy_transaction::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        kind: Set(kind),
        amount: Set(amount),
        balance_after: Set(balance_after),
        reason: Set(Some(reason.into())),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}

pub async fn get_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let acc = get_or_create_account(db, guild_id, user_id, cfg).await?;
    Ok(balance(&acc))
}

pub async fn get_leaderboard(
    db: &DatabaseConnection,
    guild_id: u64,
    limit: u64,
    offset: u64,
) -> Result<Vec<economy_account::Model>> {
    use sea_orm::sea_query::Expr;
    Ok(economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .order_by_desc(sea_orm::ExprTrait::add(
            Expr::col(economy_account::Column::Wallet),
            Expr::col(economy_account::Column::Bank),
        ))
        .order_by_asc(economy_account::Column::UserId)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?)
}

/// Count of economy accounts on the server (for leaderboard pagination).
pub async fn count_accounts(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    Ok(economy_account::Entity::find()
        .filter(economy_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .count(db)
        .await?)
}

pub async fn claim_daily(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<DailyOutcome> {
    run_with_retry!(claim_daily_inner(db, guild_id, user_id, cfg))
}

async fn claim_daily_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    cfg: &EconomyConfig,
) -> Result<DailyOutcome> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;

    if let Some(last) = acc.last_payday_at {
        let elapsed = (Utc::now().fixed_offset() - last).num_seconds();
        let remaining = cfg.payday_cooldown_seconds.saturating_sub(elapsed);
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
    let new_wallet = acc.wallet.saturating_add(amount);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.last_payday_at = Set(Some(Utc::now().fixed_offset()));
    active.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        user_id,
        EconomyTxnKind::Payday,
        amount,
        new_wallet,
        "Daily allowance",
    )
    .await?;

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
    vault: Vault,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    run_with_retry!(admin_set_balance_inner(
        db, guild_id, user_id, vault, amount, cfg
    ))
}

async fn admin_set_balance_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    vault: Vault,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;
    let amount = amount.clamp(0, cfg.max_balance);

    let (new_wallet, new_bank) = match vault {
        Vault::Bank => (acc.wallet, amount),
        Vault::Wallet => (amount, acc.bank),
    };
    let delta = new_wallet
        .saturating_add(new_bank)
        .saturating_sub(acc.wallet.saturating_add(acc.bank));
    let after = new_wallet.saturating_add(new_bank);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        user_id,
        EconomyTxnKind::AdminSet,
        delta,
        after,
        format!("Admin set {} to {amount}", vault.label()),
    )
    .await?;

    txn.commit().await?;
    Ok(AccountBalance {
        wallet: new_wallet,
        bank: new_bank,
        net_worth: after,
    })
}

pub async fn admin_adjust_balance(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    vault: Vault,
    delta: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    run_with_retry!(admin_adjust_balance_inner(
        db, guild_id, user_id, vault, delta, cfg
    ))
}

async fn admin_adjust_balance_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    vault: Vault,
    delta: i64,
    cfg: &EconomyConfig,
) -> Result<AccountBalance> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;

    let (new_wallet, new_bank) = match vault {
        Vault::Bank => (
            acc.wallet,
            acc.bank.saturating_add(delta).clamp(0, cfg.max_balance),
        ),
        Vault::Wallet => (
            acc.wallet.saturating_add(delta).clamp(0, cfg.max_balance),
            acc.bank,
        ),
    };
    let actual_delta = new_wallet
        .saturating_add(new_bank)
        .saturating_sub(acc.wallet.saturating_add(acc.bank));
    let after = new_wallet.saturating_add(new_bank);
    let kind = if delta >= 0 {
        EconomyTxnKind::AdminAdd
    } else {
        EconomyTxnKind::AdminRemove
    };

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        user_id,
        kind,
        actual_delta,
        after,
        format!("Admin adjust {} by {delta}", vault.label()),
    )
    .await?;

    txn.commit().await?;
    Ok(AccountBalance {
        wallet: new_wallet,
        bank: new_bank,
        net_worth: after,
    })
}

pub async fn play_slots(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    bid: i64,
    cfg: &EconomyConfig,
) -> std::result::Result<SlotsOutcome, SlotsError> {
    if bid < cfg.slot_min_bid || bid > cfg.slot_max_bid || bid <= 0 {
        return Err(SlotsError::InvalidBid {
            min: cfg.slot_min_bid,
            max: cfg.slot_max_bid,
        });
    }

    let mut attempt = 0;
    loop {
        attempt += 1;
        match play_slots_inner(db, guild_id, user_id, bid, cfg).await {
            Ok(outcome) => return Ok(outcome),
            Err(err) if attempt < MAX_ATTEMPTS && is_slots_conflict(&err) => {
                let backoff = INITIAL_BACKOFF_MS * (1 << (attempt - 1));
                tracing::warn!(
                    attempt,
                    backoff_ms = backoff,
                    error = %err,
                    "retrying slots transaction after conflict (40001/40P01)"
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

async fn play_slots_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    bid: i64,
    cfg: &EconomyConfig,
) -> std::result::Result<SlotsOutcome, SlotsError> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;
    if acc.wallet < bid {
        return Err(SlotsError::InsufficientFunds);
    }

    let symbols = spin_symbols();
    let multiplier = match &symbols {
        [a, b, c] if a == b && b == c => cfg.slot_payout_triple,
        [a, b, c] if a == b || b == c || a == c => cfg.slot_payout_pair,
        _ => 0,
    };
    let payout = bid.saturating_mul(multiplier).max(0);
    let after_bid = acc.wallet - bid;
    let new_wallet = after_bid.saturating_add(payout).min(cfg.max_balance);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.update(&txn).await?;

    let reason = format!("Slot machine [{}]", symbols.join(" "));
    log_transaction(
        &txn,
        guild_id,
        user_id,
        EconomyTxnKind::SlotsBid,
        -bid,
        after_bid,
        reason.clone(),
    )
    .await?;
    if payout > 0 {
        log_transaction(
            &txn,
            guild_id,
            user_id,
            EconomyTxnKind::SlotsWin,
            payout,
            new_wallet,
            reason,
        )
        .await?;
    }

    txn.commit().await?;
    Ok(SlotsOutcome {
        symbols,
        payout,
        net: payout.saturating_sub(bid),
        new_wallet,
    })
}

/// Symbols from `UUIDv4` bytes (already a dependency); unbiased enough for reels
/// and avoids the process-wide hasher used previously.
fn spin_symbols() -> [String; 3] {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    std::array::from_fn(|i| SLOT_SYMBOLS[usize::from(bytes[i]) % SLOT_SYMBOLS.len()].to_string())
}

pub async fn transfer(
    db: &DatabaseConnection,
    guild_id: u64,
    sender_id: u64,
    recipient_id: u64,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<Option<i64>> {
    if sender_id == recipient_id {
        return Ok(None);
    }
    run_with_retry!(transfer_inner(
        db,
        guild_id,
        sender_id,
        recipient_id,
        amount,
        cfg
    ))
}

async fn transfer_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    sender_id: u64,
    recipient_id: u64,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<Option<i64>> {
    let txn = db.begin().await?;

    // Lock both rows in a deterministic order so A→B and B→A cannot deadlock.
    let (first, second) = if sender_id <= recipient_id {
        (sender_id, recipient_id)
    } else {
        (recipient_id, sender_id)
    };
    let first_acc = locked_account(&txn, guild_id, first, cfg).await?;
    let second_acc = locked_account(&txn, guild_id, second, cfg).await?;
    let (sender, recipient) = if first == sender_id {
        (first_acc, second_acc)
    } else {
        (second_acc, first_acc)
    };

    if sender.wallet < amount {
        return Ok(None);
    }

    let tax = cfg.transfer_tax(amount);
    let room = cfg.max_balance.saturating_sub(recipient.wallet).max(0);
    let credited = amount.saturating_sub(tax).min(room);

    let sender_new = sender.wallet - amount;
    let recipient_new = recipient.wallet.saturating_add(credited);

    let mut sender_act: economy_account::ActiveModel = sender.into();
    sender_act.wallet = Set(sender_new);
    sender_act.update(&txn).await?;

    let mut recipient_act: economy_account::ActiveModel = recipient.into();
    recipient_act.wallet = Set(recipient_new);
    recipient_act.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        sender_id,
        EconomyTxnKind::TransferOut,
        -amount,
        sender_new,
        format!("Transfer to {recipient_id}"),
    )
    .await?;
    log_transaction(
        &txn,
        guild_id,
        recipient_id,
        EconomyTxnKind::TransferIn,
        credited,
        recipient_new,
        format!("Transfer from {sender_id}"),
    )
    .await?;

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
    run_with_retry!(deposit_inner(db, guild_id, user_id, amount, cfg))
}

async fn deposit_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;

    let moved = amount
        .min(acc.wallet)
        .min(cfg.max_balance.saturating_sub(acc.bank))
        .max(0);
    if moved <= 0 {
        return Ok(None);
    }

    let new_wallet = acc.wallet - moved;
    let new_bank = acc.bank.saturating_add(moved);

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        user_id,
        EconomyTxnKind::TransferOut,
        -moved,
        new_wallet,
        "Bank deposit",
    )
    .await?;

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
    run_with_retry!(withdraw_inner(db, guild_id, user_id, amount, cfg))
}

async fn withdraw_inner(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    cfg: &EconomyConfig,
) -> Result<Option<(i64, i64)>> {
    let txn = db.begin().await?;
    let acc = locked_account(&txn, guild_id, user_id, cfg).await?;

    let moved = amount
        .min(acc.bank)
        .min(cfg.max_balance.saturating_sub(acc.wallet))
        .max(0);
    if moved <= 0 {
        return Ok(None);
    }

    let new_wallet = acc.wallet.saturating_add(moved);
    let new_bank = acc.bank - moved;

    let mut active: economy_account::ActiveModel = acc.into();
    active.wallet = Set(new_wallet);
    active.bank = Set(new_bank);
    active.update(&txn).await?;

    log_transaction(
        &txn,
        guild_id,
        user_id,
        EconomyTxnKind::TransferIn,
        moved,
        new_wallet,
        "Bank withdrawal",
    )
    .await?;

    txn.commit().await?;
    Ok(Some((new_wallet, new_bank)))
}

#[cfg(test)]
mod tests {
    use super::{Vault, is_conflict_db_err, is_conflict_error};
    use lumi_kernel::LumiError;

    #[test]
    fn parses_vault() {
        assert_eq!(Vault::parse(None), Some(Vault::Wallet));
        assert_eq!(Vault::parse(Some("wallet")), Some(Vault::Wallet));
        assert_eq!(Vault::parse(Some("bank")), Some(Vault::Bank));
        assert_eq!(Vault::parse(Some("other")), None);
    }

    #[test]
    fn detects_conflict_errors() {
        let serialize_err = sea_orm::DbErr::Custom("could not serialize access: 40001".into());
        let deadlock_err = sea_orm::DbErr::Custom("deadlock detected: 40P01".into());
        let err_other = sea_orm::DbErr::Custom("other error".into());

        assert!(is_conflict_db_err(&serialize_err));
        assert!(is_conflict_db_err(&deadlock_err));
        assert!(!is_conflict_db_err(&err_other));

        assert!(is_conflict_error(&LumiError::Db(serialize_err)));
        assert!(is_conflict_error(&LumiError::Db(deadlock_err)));
        assert!(!is_conflict_error(&LumiError::Db(err_other)));
    }
}
