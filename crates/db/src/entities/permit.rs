use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "permits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub name: String,
    pub kind: enums::PermitKind,
    pub polarity: enums::PermitPolarity,
    pub nodes: Vec<String>,
    pub builtin: bool,
    pub position: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
