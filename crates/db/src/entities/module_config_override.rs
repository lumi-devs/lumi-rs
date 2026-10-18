use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "module_config_overrides")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub module_name: String,
    pub key: String,
    pub model_type: enums::OverrideTargetType,
    pub model_id: String,
    pub value: Json,
}

impl ActiveModelBehavior for ActiveModel {}
