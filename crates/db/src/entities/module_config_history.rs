use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "module_config_history")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub module_name: String,
    pub key: String,
    pub old_value: Option<Json>,
    pub new_value: Json,
    pub actor_id: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
