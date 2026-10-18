use sea_orm::entity::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitKind {
    #[sea_orm(string_value = "enforced")]
    Enforced,
    #[sea_orm(string_value = "custom")]
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitPolarity {
    #[sea_orm(string_value = "grant")]
    Grant,
    #[sea_orm(string_value = "deny")]
    Deny,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum CaseAction {
    #[sea_orm(string_value = "ban")]
    Ban,
    #[sea_orm(string_value = "unban")]
    Unban,
    #[sea_orm(string_value = "kick")]
    Kick,
    #[sea_orm(string_value = "mute")]
    Mute,
    #[sea_orm(string_value = "unmute")]
    Unmute,
    #[sea_orm(string_value = "softban")]
    Softban,
    #[sea_orm(string_value = "voice_mute")]
    VoiceMute,
    #[sea_orm(string_value = "unvoice_mute")]
    UnvoiceMute,
    #[sea_orm(string_value = "warn")]
    Warn,
    #[sea_orm(string_value = "quarantine")]
    Quarantine,
    #[sea_orm(string_value = "unquarantine")]
    Unquarantine,
    #[sea_orm(string_value = "antinuke_alert")]
    AntinukeAlert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum AppealStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "approved")]
    Approved,
    #[sea_orm(string_value = "denied")]
    Denied,
    #[sea_orm(string_value = "denied_blacklisted")]
    DeniedBlacklisted,
    #[sea_orm(string_value = "dismissed")]
    Dismissed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum OverrideTargetType {
    #[sea_orm(string_value = "channel")]
    Channel,
    #[sea_orm(string_value = "category")]
    Category,
    #[sea_orm(string_value = "role")]
    Role,
    #[sea_orm(string_value = "user")]
    User,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum AuditPlatform {
    #[sea_orm(string_value = "discord")]
    Discord,
    #[sea_orm(string_value = "web")]
    Web,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum GdprExportJobStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "running")]
    Running,
    #[sea_orm(string_value = "done")]
    Done,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum EconomyTxnKind {
    #[sea_orm(string_value = "deposit")]
    Deposit,
    #[sea_orm(string_value = "withdraw")]
    Withdraw,
    #[sea_orm(string_value = "transfer_out")]
    TransferOut,
    #[sea_orm(string_value = "transfer_in")]
    TransferIn,
    #[sea_orm(string_value = "slots_bid")]
    SlotsBid,
    #[sea_orm(string_value = "slots_win")]
    SlotsWin,
    #[sea_orm(string_value = "payday")]
    Payday,
    #[sea_orm(string_value = "admin_add")]
    AdminAdd,
    #[sea_orm(string_value = "admin_remove")]
    AdminRemove,
    #[sea_orm(string_value = "admin_set")]
    AdminSet,
}

