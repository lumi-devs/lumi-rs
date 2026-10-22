//! Downloader addon repositories.

use sea_orm::entity::prelude::*;

/// Database model for a downloader addon repository.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "downloader_repos")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Repository name.
    #[sea_orm(unique)]
    pub name: String,
    /// Git clone URL.
    pub url: String,
    /// Git branch to follow.
    pub branch: String,
    /// Last pulled git commit SHA.
    pub commit: Option<String>,
    /// Signing key identity or signature info.
    pub signed_by: Option<String>,
    /// Timestamp when the repository was added.
    pub created_at: DateTimeWithTimeZone,
    /// Timestamp when the repository was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
