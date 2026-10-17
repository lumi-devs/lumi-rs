use sea_orm::entity::prelude::*;

/// Per-guild, per-module, per-key config value (the generic module config store).
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub module: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: Json,
}

impl ActiveModelBehavior for ActiveModel {}
