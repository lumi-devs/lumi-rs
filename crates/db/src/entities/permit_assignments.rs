use sea_orm::entity::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitTargetType {
    #[sea_orm(string_value = "user")]
    User,
    #[sea_orm(string_value = "role")]
    Role,
    #[sea_orm(string_value = "channel")]
    Channel,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permit_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub permit_id: i32,
    #[sea_orm(indexed)]
    pub guild_id: i64,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub target_type: PermitTargetType,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub target_id: i64,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
