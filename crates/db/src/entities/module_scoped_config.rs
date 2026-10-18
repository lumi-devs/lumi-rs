use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "module_scoped_config")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub scope: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub scope_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub config_key: String,
    pub value: Json,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
