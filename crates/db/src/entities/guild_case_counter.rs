use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_case_counter")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    pub next: i32,
}

impl ActiveModelBehavior for ActiveModel {}
