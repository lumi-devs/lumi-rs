use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "module_dynamic_data")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: Json,
    pub expires_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
