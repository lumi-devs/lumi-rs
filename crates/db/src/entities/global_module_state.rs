//! Global module enable/disable states.

use sea_orm::entity::prelude::*;

/// Database model for global module state.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "global_module_state")]
pub struct Model {
    /// Module name.
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    /// Whether the module is enabled globally.
    pub enabled: bool,
    /// Optional reason for the module state.
    pub reason: Option<String>,
    /// Timestamp when the state was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
