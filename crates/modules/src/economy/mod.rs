use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::shared::{int_field, string_field};

pub mod commands;
pub mod config;
pub mod store;

pub use commands::handle_component;

#[derive(Debug)]
pub struct EconomyModule;

impl Module for EconomyModule {
    fn name(&self) -> &'static str {
        "economy"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Economy".to_string(),
            emoji: "🪙".to_string(),
            description: "Server currency system with wallets, daily allowances, and transfers."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            string_field(
                "currency_name",
                "Currency Name",
                "What the currency is called (shown after amounts).",
                "credits",
                None,
            ),
            string_field(
                "currency_emoji",
                "Currency Emoji",
                "Emoji shown before amounts.",
                "🪙",
                None,
            ),
            string_field(
                "bank_name",
                "Bank Label",
                "What the bank balance is called.",
                "Bank",
                None,
            ),
            int_field(
                "starting_wallet",
                "Starting Wallet",
                "Wallet balance for new accounts.",
                100,
                None,
            ),
            int_field(
                "starting_bank",
                "Starting Bank",
                "Bank balance for new accounts.",
                0,
                None,
            ),
            int_field(
                "max_balance",
                "Maximum Balance",
                "Cap applied to both wallet and bank.",
                1_000_000,
                None,
            ),
            int_field(
                "payday_amount",
                "Payday Amount",
                "Currency granted by /payday.",
                120,
                None,
            ),
            int_field(
                "payday_cooldown_seconds",
                "Payday Cooldown (seconds)",
                "Wait between /payday claims.",
                300,
                None,
            ),
            int_field(
                "transfer_tax_percent",
                "Transfer Tax (%)",
                "Percent withheld from /pay transfers.",
                0,
                None,
            ),
            int_field(
                "slot_min_bid",
                "Slots Minimum Bid",
                "Smallest allowed /slots bet.",
                5,
                None,
            ),
            int_field(
                "slot_max_bid",
                "Slots Maximum Bid",
                "Largest allowed /slots bet.",
                100,
                None,
            ),
            int_field(
                "slot_cooldown_seconds",
                "Slots Cooldown (seconds)",
                "Wait between /slots spins.",
                5,
                None,
            ),
            int_field(
                "slot_payout_triple",
                "Slots Triple Payout (x)",
                "Multiplier for three matching symbols.",
                5,
                None,
            ),
            int_field(
                "slot_payout_pair",
                "Slots Pair Payout (x)",
                "Multiplier for two matching symbols.",
                2,
                None,
            ),
        ]
    }
}
