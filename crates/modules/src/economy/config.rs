use lumi_kernel::module_state;
use lumi_kernel::Result;
use sea_orm::DatabaseConnection;

#[derive(Clone, Debug)]
pub struct EconomyConfig {
    pub currency_name: String,
    pub currency_emoji: String,
    pub bank_name: String,
}

impl Default for EconomyConfig {
    fn default() -> Self {
        Self {
            currency_name: "credits".to_string(),
            currency_emoji: "🪙".to_string(),
            bank_name: "Bank".to_string(),
        }
    }
}

impl EconomyConfig {
    pub async fn load(db: &DatabaseConnection, guild_id: u64) -> Result<Self> {
        let mut cfg = Self::default();
        if let Some(v) = non_empty(module_state::config_value(db, guild_id, "economy", "currency_name").await?) {
            cfg.currency_name = v;
        }
        if let Some(v) = non_empty(module_state::config_value(db, guild_id, "economy", "currency_emoji").await?) {
            cfg.currency_emoji = v;
        }
        if let Some(v) = non_empty(module_state::config_value(db, guild_id, "economy", "bank_name").await?) {
            cfg.bank_name = v;
        }
        Ok(cfg)
    }

    pub fn format_amount(&self, amount: i64) -> String {
        format!("{} {} {}", self.currency_emoji, group_digits(amount), self.currency_name)
    }
}

fn non_empty(value: Option<serde_json::Value>) -> Option<String> {
    value
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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
    use super::group_digits;

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(1234567), "1,234,567");
        assert_eq!(group_digits(-42000), "-42,000");
    }
}
