use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tempvc_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: String,
    pub owner_id: String,
    pub generator_id: String,
    pub name: String,
    pub number: i32,
    pub locked: bool,
    pub hidden: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
