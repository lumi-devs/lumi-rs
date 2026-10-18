use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mod_notes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub user_id: String,
    pub author_id: String,
    pub message: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
