use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "gdpr_export_jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub user_id: String,
    pub requested_by: String,
    pub status: enums::GdprExportJobStatus,
    pub error: Option<String>,
    pub file_path: Option<String>,
    pub size_bytes: Option<i32>,
    pub created_at: DateTimeWithTimeZone,
    pub completed_at: Option<DateTimeWithTimeZone>,
    pub expires_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
