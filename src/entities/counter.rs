use sea_orm::entity::prelude::*;

/// A named, per-guild monotonically increasing counter (e.g. moderation case
/// numbers). Incremented atomically through `db::next_counter`.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "counters")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    pub value: i64,
}

impl ActiveModelBehavior for ActiveModel {}
