use lumi_kernel::Result;
use lumi_kernel::module_state;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use crate::shared::group_digits;

#[derive(Clone, Debug)]
pub struct EconomyConfig {
    pub currency_name: String,
    pub currency_emoji: String,
    pub bank_name: String,
    pub starting_wallet: i64,
    pub starting_bank: i64,
    pub max_balance: i64,
    pub payday_amount: i64,
    pub payday_cooldown_seconds: i64,
    pub transfer_tax_percent: i64,
    pub slot_min_bid: i64,
    pub slot_max_bid: i64,
    pub slot_cooldown_seconds: i64,
    pub slot_payout_triple: i64,
    pub slot_payout_pair: i64,
}

impl Default for EconomyConfig {
    fn default() -> Self {
        Self {
            currency_name: "credits".to_string(),
            currency_emoji: "🪙".to_string(),
            bank_name: "Bank".to_string(),
            starting_wallet: 100,
            starting_bank: 0,
            max_balance: 1_000_000,
            payday_amount: 120,
            payday_cooldown_seconds: 300,
            transfer_tax_percent: 0,
            slot_min_bid: 5,
            slot_max_bid: 100,
            slot_cooldown_seconds: 5,
            slot_payout_triple: 5,
            slot_payout_pair: 2,
        }
    }
}

impl EconomyConfig {
    /// Load the whole module config in one query, then apply values in memory.
    pub async fn load(db: &DatabaseConnection, guild_id: u64) -> Result<Self> {
        let values = module_state::get_config(db, guild_id, "economy").await;
        let mut cfg = Self::default();

        if let Some(v) = string_value(&values, "currency_name") {
            cfg.currency_name = v;
        }
        if let Some(v) = string_value(&values, "currency_emoji") {
            cfg.currency_emoji = v;
        }
        if let Some(v) = string_value(&values, "bank_name") {
            cfg.bank_name = v;
        }
        set_i64(&values, "starting_wallet", &mut cfg.starting_wallet, 0, i64::MAX);
        set_i64(&values, "starting_bank", &mut cfg.starting_bank, 0, i64::MAX);
        set_i64(&values, "max_balance", &mut cfg.max_balance, 1, i64::MAX);
        set_i64(&values, "payday_amount", &mut cfg.payday_amount, 0, i64::MAX);
        set_i64(
            &values,
            "payday_cooldown_seconds",
            &mut cfg.payday_cooldown_seconds,
            0,
            86_400,
        );
        set_i64(
            &values,
            "transfer_tax_percent",
            &mut cfg.transfer_tax_percent,
            0,
            100,
        );
        set_i64(&values, "slot_min_bid", &mut cfg.slot_min_bid, 1, i64::MAX);
        set_i64(&values, "slot_max_bid", &mut cfg.slot_max_bid, 1, i64::MAX);
        set_i64(
            &values,
            "slot_cooldown_seconds",
            &mut cfg.slot_cooldown_seconds,
            0,
            86_400,
        );
        set_i64(
            &values,
            "slot_payout_triple",
            &mut cfg.slot_payout_triple,
            1,
            1000,
        );
        set_i64(
            &values,
            "slot_payout_pair",
            &mut cfg.slot_payout_pair,
            1,
            1000,
        );
        cfg.slot_max_bid = cfg.slot_max_bid.max(cfg.slot_min_bid);
        cfg.starting_wallet = cfg.starting_wallet.min(cfg.max_balance);
        cfg.starting_bank = cfg.starting_bank.min(cfg.max_balance);
        Ok(cfg)
    }

    pub fn format_amount(&self, amount: i64) -> String {
        format!(
            "{} {} {}",
            self.currency_emoji,
            group_digits(amount),
            self.currency_name
        )
    }

    /// Tax (in whole units) withheld from a transfer of `amount`.
    pub fn transfer_tax(&self, amount: i64) -> i64 {
        if self.transfer_tax_percent <= 0 || amount <= 0 {
            return 0;
        }
        let tax = i128::from(amount) * i128::from(self.transfer_tax_percent) / 100;
        i64::try_from(tax).unwrap_or(i64::MAX)
    }

    /// Milliseconds for a cooldown, never negative.
    pub fn cooldown_ms(&self, seconds: i64) -> u64 {
        u64::try_from(seconds.max(0).saturating_mul(1000)).unwrap_or(u64::MAX)
    }
}

fn string_value(values: &Map<String, Value>, key: &str) -> Option<String> {
    values
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Read a JSON number or numeric string, clamping to `[min, max]`.
fn set_i64(values: &Map<String, Value>, key: &str, slot: &mut i64, min: i64, max: i64) {
    let Some(n) = values.get(key).and_then(value_as_i64) else {
        return;
    };
    *slot = n.clamp(min, max);
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::EconomyConfig;

    #[test]
    fn transfer_tax_rounds_down_and_cannot_overflow() {
        let cfg = EconomyConfig {
            transfer_tax_percent: 5,
            ..EconomyConfig::default()
        };
        assert_eq!(cfg.transfer_tax(1000), 50);
        assert_eq!(cfg.transfer_tax(99), 4);
        assert_eq!(cfg.transfer_tax(-5), 0);
        assert_eq!(EconomyConfig::default().transfer_tax(1000), 0);

        let extreme = EconomyConfig {
            transfer_tax_percent: 100,
            ..EconomyConfig::default()
        };
        assert_eq!(extreme.transfer_tax(i64::MAX), i64::MAX);
    }

    #[test]
    fn cooldown_ms_is_non_negative() {
        let cfg = EconomyConfig::default();
        assert_eq!(cfg.cooldown_ms(-1), 0);
        assert_eq!(cfg.cooldown_ms(3), 3_000);
    }
}
