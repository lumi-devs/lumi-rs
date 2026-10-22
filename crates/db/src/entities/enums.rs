//! Database enum types.

use sea_orm::entity::prelude::*;

/// Kind of permit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitKind {
    /// Enforced permit.
    #[sea_orm(string_value = "enforced")]
    Enforced,
    /// Custom permit.
    #[sea_orm(string_value = "custom")]
    Custom,
}

/// Permit grant or deny polarity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitPolarity {
    /// Allow action.
    #[sea_orm(string_value = "grant")]
    Grant,
    /// Deny action.
    #[sea_orm(string_value = "deny")]
    Deny,
}

/// Target type for permit assignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum PermitTargetType {
    /// User target.
    #[sea_orm(string_value = "user")]
    User,
    /// Role target.
    #[sea_orm(string_value = "role")]
    Role,
    /// Channel target.
    #[sea_orm(string_value = "channel")]
    Channel,
}

/// Moderation case action type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum CaseAction {
    /// Ban action.
    #[sea_orm(string_value = "ban")]
    Ban,
    /// Unban action.
    #[sea_orm(string_value = "unban")]
    Unban,
    /// Kick action.
    #[sea_orm(string_value = "kick")]
    Kick,
    /// Mute action.
    #[sea_orm(string_value = "mute")]
    Mute,
    /// Unmute action.
    #[sea_orm(string_value = "unmute")]
    Unmute,
    /// Softban action.
    #[sea_orm(string_value = "softban")]
    Softban,
    /// Voice mute action.
    #[sea_orm(string_value = "voice_mute")]
    VoiceMute,
    /// Voice unmute action.
    #[sea_orm(string_value = "unvoice_mute")]
    UnvoiceMute,
    /// Warning action.
    #[sea_orm(string_value = "warn")]
    Warn,
    /// Quarantine action.
    #[sea_orm(string_value = "quarantine")]
    Quarantine,
    /// Unquarantine action.
    #[sea_orm(string_value = "unquarantine")]
    Unquarantine,
    /// Antinuke alert action.
    #[sea_orm(string_value = "antinuke_alert")]
    AntinukeAlert,
}

/// Economy transaction kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum EconomyTxnKind {
    /// Slots bid.
    #[sea_orm(string_value = "slots_bid")]
    SlotsBid,
    /// Slots win.
    #[sea_orm(string_value = "slots_win")]
    SlotsWin,
    /// Periodic payday.
    #[sea_orm(string_value = "payday")]
    Payday,
    /// Admin balance addition.
    #[sea_orm(string_value = "admin_add")]
    AdminAdd,
    /// Admin balance removal.
    #[sea_orm(string_value = "admin_remove")]
    AdminRemove,
    /// Admin balance set.
    #[sea_orm(string_value = "admin_set")]
    AdminSet,
    /// Transfer out to another user.
    #[sea_orm(string_value = "transfer_out")]
    TransferOut,
    /// Transfer in from another user.
    #[sea_orm(string_value = "transfer_in")]
    TransferIn,
}
