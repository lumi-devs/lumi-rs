use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "guilds")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub prefix: Option<String>,
    pub locale: String,
    pub ignored: bool,
    pub left_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
