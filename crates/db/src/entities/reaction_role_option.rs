use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "reactionrole_options")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub menu_id: String,
    pub slug: String,
    pub position: i32,
    pub label: String,
    pub emoji: Option<String>,
    pub description: Option<String>,
    pub role_id: String,
    pub required_role_id: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
