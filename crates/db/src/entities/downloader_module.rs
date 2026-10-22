//! Installed downloader modules.

use sea_orm::entity::prelude::*;

/// Database model for an installed downloader module.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "downloader_modules")]
pub struct Model {
    /// Repository identifier.
    #[sea_orm(primary_key, auto_increment = false)]
    pub repo_id: i32,
    /// Module name.
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    /// Installed module version.
    pub version: Option<String>,
    /// Git commit SHA of the installed module.
    pub commit: Option<String>,
    /// Signing key identity or signature info.
    pub signed_by: Option<String>,
    /// Whether the module is pinned against auto-updates.
    pub pinned: bool,
    /// Timestamp when the module was installed.
    pub installed_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
