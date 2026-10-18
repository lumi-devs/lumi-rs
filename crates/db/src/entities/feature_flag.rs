use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "feature_flags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rollout_percent: i32,
    pub updated_at: DateTimeWithTimeZone,
    pub updated_by: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
