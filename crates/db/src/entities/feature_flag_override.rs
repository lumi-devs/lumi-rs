use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "feature_flag_overrides")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub flag_key: String,
    pub guild_id: String,
    pub enabled: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
