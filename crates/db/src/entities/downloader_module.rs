use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "downloader_modules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub repo_id: i32,
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    pub version: Option<String>,
    pub commit: Option<String>,
    pub signed_by: Option<String>,
    pub pinned: bool,
    pub installed_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
