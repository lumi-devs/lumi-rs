use lumi_kernel::module_state;
use lumi_kernel::Result;
use sea_orm::DatabaseConnection;
use serde_json::Value;

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
    pub async fn load(db: &DatabaseConnection, guild_id: u64) -> Result<Self> {
        let mut cfg = Self::default();
        let get = |key: &'static str| module_state::config_value(db, guild_id, "economy", key);
        if let Some(v) = non_empty(get("currency_name").await?) {
            cfg.currency_name = v;
        }
        if let Some(v) = non_empty(get("currency_emoji").await?) {
            cfg.currency_emoji = v;
        }
        if let Some(v) = non_empty(get("bank_name").await?) {
            cfg.bank_name = v;
        }
        set_i64(&mut cfg.starting_wallet, get("starting_wallet").await?, 0, i64::MAX);
        set_i64(&mut cfg.starting_bank, get("starting_bank").await?, 0, i64::MAX);
        set_i64(&mut cfg.max_balance, get("max_balance").await?, 1, i64::MAX);
        set_i64(&mut cfg.payday_amount, get("payday_amount").await?, 0, i64::MAX);
        set_i64(
            &mut cfg.payday_cooldown_seconds,
            get("payday_cooldown_seconds").await?,
            0,
            86_400,
        );
        set_i64(&mut cfg.transfer_tax_percent, get("transfer_tax_percent").await?, 0, 100);
        set_i64(&mut cfg.slot_min_bid, get("slot_min_bid").await?, 1, i64::MAX);
        set_i64(&mut cfg.slot_max_bid, get("slot_max_bid").await?, 1, i64::MAX);
        set_i64(
            &mut cfg.slot_cooldown_seconds,
            get("slot_cooldown_seconds").await?,
            0,
            86_400,
        );
        set_i64(&mut cfg.slot_payout_triple, get("slot_payout_triple").await?, 1, 1000);
        set_i64(&mut cfg.slot_payout_pair, get("slot_payout_pair").await?, 1, 1000);
        if cfg.slot_max_bid < cfg.slot_min_bid {
            cfg.slot_max_bid = cfg.slot_min_bid;
        }
        Ok(cfg)
    }

    pub fn format_amount(&self, amount: i64) -> String {
        format!("{} {} {}", self.currency_emoji, group_digits(amount), self.currency_name)
    }

    /// Tax (in whole units) withheld from a transfer of `amount`.
    pub fn transfer_tax(&self, amount: i64) -> i64 {
        if self.transfer_tax_percent <= 0 {
            0
        } else {
            (amount * self.transfer_tax_percent / 100).max(0)
        }
    }
}

fn non_empty(value: Option<Value>) -> Option<String> {
    value
        .as_ref()
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Read a JSON number or numeric string, clamping to `[min, max]`.
fn set_i64(slot: &mut i64, value: Option<Value>, min: i64, max: i64) {
    let Some(n) = value.as_ref().and_then(value_as_i64) else {
        return;
    };
    *slot = n.clamp(min, max);
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
}

fn group_digits(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{group_digits, EconomyConfig};

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(1234567), "1,234,567");
        assert_eq!(group_digits(-42000), "-42,000");
    }

    #[test]
    fn transfer_tax_rounds_down() {
        let cfg = EconomyConfig {
            transfer_tax_percent: 5,
            ..EconomyConfig::default()
        };
        assert_eq!(cfg.transfer_tax(1000), 50);
        assert_eq!(cfg.transfer_tax(99), 4);
        assert_eq!(EconomyConfig::default().transfer_tax(1000), 0);
    }
}
