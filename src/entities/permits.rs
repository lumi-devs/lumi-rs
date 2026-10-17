use sea_orm::entity::prelude::*;

use crate::permits::{PermitKind, PermitPolarity};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(indexed, unique_key = "uq_permit_guild_name")]
    pub guild_id: i64,
    #[sea_orm(unique_key = "uq_permit_guild_name")]
    pub name: String,
    pub kind: PermitKind,
    pub polarity: PermitPolarity,
    pub nodes: Vec<String>,
    pub builtin: bool,
    pub position: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
