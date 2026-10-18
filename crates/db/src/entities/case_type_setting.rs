use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "case_type_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    pub enabled: bool,
    pub channel_id: Option<String>,
    pub emoji: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
