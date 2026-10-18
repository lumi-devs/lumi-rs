use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_backups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub data: Json,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
