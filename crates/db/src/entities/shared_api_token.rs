use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "shared_api_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub service: String,
    pub token: String,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
