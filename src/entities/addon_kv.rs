use sea_orm::entity::prelude::*;

/// Per-guild, per-addon key/value store. The backing store for the addon
/// `kv` capability.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "addon_kv")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub addon: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: Json,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
