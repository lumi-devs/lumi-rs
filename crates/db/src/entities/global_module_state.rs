use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "global_module_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    pub enabled: bool,
    pub reason: Option<String>,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
