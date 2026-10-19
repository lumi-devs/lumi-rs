use sea_orm::entity::prelude::*;
use crate::ids::GuildId;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "guilds")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: GuildId,
    pub prefix: Option<String>,
    pub locale: String,
    pub ignored: bool,
    #[sea_orm(indexed)]
    pub left_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
