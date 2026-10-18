use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "reactionrole_menus")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub guild_id: String,
    pub title: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub mode: String,
    pub exclusive: bool,
    pub max_roles: i32,
    pub channel_id: Option<String>,
    pub message_ids: Vec<String>,
    pub rich_content: Json,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
