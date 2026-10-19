use poise::serenity_prelude as serenity;
use serenity::all::{
    ComponentInteraction, CreateAllowedMentions,
    CreateInteractionResponse, CreateInteractionResponseMessage, CreateModal, GuildId, ModalInteraction,
    MessageFlags,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_kernel::permits::CheckContext;
use lumi_kernel::Services;
use lumi_ui::CardReply;

pub mod addons;
pub mod home;
pub mod modules;
pub mod permissions;
pub mod settings;

/// Every hub custom id starts with this.
pub const PREFIX: &str = "lumi";
/// Fixed custom id of the set-prefix modal.
pub const PREFIX_MODAL_ID: &str = "lumi:prefixmodal";
/// Prefix of the addon management modals.
pub const ADDON_MODAL_ID: &str = "lumi:addonmodal";

/// The permit required to use any hub management action.
pub const ADMIN_NODE: &str = "admin.*";

/// A hub tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HubView {
    Home,
    Modules,
    Permissions,
    Settings,
    Addons,
}

impl HubView {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Modules => "modules",
            Self::Permissions => "permissions",
            Self::Settings => "settings",
            Self::Addons => "addons",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "home" => Some(Self::Home),
            "modules" => Some(Self::Modules),
            "permissions" => Some(Self::Permissions),
            "settings" => Some(Self::Settings),
            "addons" => Some(Self::Addons),
            _ => None,
        }
    }
}

/// Guild context captured once per interaction.
#[derive(Clone, Debug)]
pub struct GuildCtx {
    pub id: u64,
    pub name: String,
    pub icon: Option<String>,
}

/// A parsed `lumi:` custom id.
#[derive(Debug)]
pub struct HubId {
    pub action: String,
    pub tail: Vec<String>,
}

impl HubId {
    pub fn parse(custom_id: &str) -> Option<Self> {
        let mut parts = custom_id.split(':');
        if parts.next()? != PREFIX {
            return None;
        }
        let action = parts.next()?.to_string();
        Some(Self {
            action,
            tail: parts.map(str::to_string).collect(),
        })
    }

    pub fn sub(&self) -> Option<&str> {
        self.tail.first().map(String::as_str)
    }

    pub fn rest(&self, from: usize) -> String {
        self.tail[from.min(self.tail.len())..].join(":")
    }
}

/// Build a `lumi:` custom id from segments.
pub fn cid(parts: &[&str]) -> String {
    format!("{PREFIX}:{}", parts.join(":"))
}

/// What a dispatched action produced.
#[derive(Debug)]
pub enum HubOutcome {
    /// Replace the panel message.
    Update(CardReply),
    /// Send an ephemeral follow-up card.
    Ephemeral(CardReply),
    /// Open a modal.
    Modal(CreateModal<'static>),
    /// Nothing to render.
    Done,
}

/// Root `/lumi` and `/lumi panel` entry point.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    subcommands("panel")
)]
pub async fn lumi(ctx: Context<'_>) -> Result<(), Error> {
    open(ctx).await
}

/// Open the interactive control panel.
#[poise::command(slash_command, prefix_command, guild_only, rename = "panel")]
pub async fn panel(ctx: Context<'_>) -> Result<(), Error> {
    open(ctx).await
}

pub fn commands() -> Vec<Command> {
    vec![lumi()]
}

async fn open(ctx: Context<'_>) -> Result<(), Error> {
    let ctx_guild = {
        let guild = ctx
            .guild()
            .ok_or_else(|| anyhow::anyhow!("this command is guild only"))?;
        GuildCtx {
            id: guild.id.get(),
            name: guild.name.to_string(),
            icon: guild.icon_url(),
        }
    };
    let card = home::card(&ctx.data(), &ctx_guild).await?;
    match ctx {
        poise::Context::Application(app) => {
            app.interaction
                .create_response(
                    ctx.http(),
                    CreateInteractionResponse::Message(v2_message(card.components)),
                )
                .await?;
        }
        poise::Context::Prefix(pfx) => {
            pfx.msg
                .channel_id
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new()
                        .components(card.components)
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .allowed_mentions(CreateAllowedMentions::new()),
                )
                .await?;
        }
    }
    Ok(())
}

/// Handle any `lumi:` button or select interaction.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    let Some(parsed) = HubId::parse(&component.data.custom_id) else {
        return Ok(());
    };
    let Some(raw_guild_id) = component.guild_id else {
        return Ok(());
    };
    if parsed.action == "indicator" {
        return Ok(());
    }
    let guild = guild_ctx(ctx, raw_guild_id);

    // Prefix and addon modals open without a permission round-trip; the modal
    // submission re-checks. Everything else requires `admin.*`.
    let opens_modal =
        (parsed.action == "prefix" && parsed.sub() == Some("set"))
            || (parsed.action == "addon"
                && matches!(parsed.sub(), Some("add_repo" | "rm_repo" | "install" | "uninstall")));

    if opens_modal {
        if !caller_may_manage(ctx, services, component).await {
            return deny(ctx, component).await;
        }
        let outcome = dispatch(services, &guild, &parsed, &component.data, component.user.id.get())
            .await?;
        return respond_outcome(ctx, component, outcome).await;
    }

    acknowledge(ctx, component).await?;

    if !caller_may_manage(ctx, services, component).await {
        return deny(ctx, component).await;
    }

    let outcome =
        dispatch(services, &guild, &parsed, &component.data, component.user.id.get()).await?;
    respond_outcome(ctx, component, outcome).await
}

/// Handle the `lumi:prefixmodal` and `lumi:addonmodal:*` submissions.
pub async fn handle_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    if modal.data.custom_id == PREFIX_MODAL_ID {
        if !caller_may_manage_modal(ctx, services, modal).await {
            return deny_modal(ctx, modal).await;
        }
        let guild = modal.guild_id.map_or(
            GuildCtx { id: 0, name: "this server".to_string(), icon: None },
            |id| guild_ctx(ctx, id),
        );
        let outcome = settings::handle_prefix_modal(services, &guild, modal).await?;
        return respond_modal(ctx, modal, outcome).await;
    }
    if let Some(parsed) = HubId::parse(&modal.data.custom_id) {
        if parsed.action == "addonmodal" {
            if !services.cfg.is_owner(modal.user.id.get()) {
                return deny_modal(ctx, modal).await;
            }
            let guild = modal.guild_id.map_or(
                GuildCtx { id: 0, name: "this server".to_string(), icon: None },
                |id| guild_ctx(ctx, id),
            );
            let outcome =
                addons::handle_modal(services, &guild, modal, parsed.sub().unwrap_or_default()).await?;
            return respond_modal(ctx, modal, outcome).await;
        }
    }
    Ok(())
}

async fn dispatch(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match (id.action.as_str(), id.sub()) {
        ("home", _) | ("tab", Some("home")) => Ok(HubOutcome::Update(home::card(services, guild).await?)),
        ("tab", Some("modules")) => Ok(HubOutcome::Update(modules::list_card(services, guild, 0, user_id).await?)),
        ("tab", Some("permissions")) => Ok(HubOutcome::Update(permissions::list_card(services, guild, 0, user_id).await?)),
        ("tab", Some("settings")) => Ok(HubOutcome::Update(settings::card(services, guild, user_id).await?)),
        ("tab", Some("addons")) => Ok(HubOutcome::Update(addons::dashboard_card(services, guild, user_id).await?)),
        _ => dispatch_ext(services, guild, id, data, user_id).await,
    }
}

async fn dispatch_ext(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match id.action.as_str() {
        "prefix" => match id.sub() {
            Some("set") => Ok(HubOutcome::Modal(settings::prefix_modal())),
            Some("reset") => {
                lumi_db::reset_prefix(&services.db, guild.id).await?;
                Ok(HubOutcome::Update(settings::card(services, guild, user_id).await?))
            }
            _ => Ok(HubOutcome::Done),
        },
        _ if id.action.starts_with("perm") || id.action == "permit" => {
            permissions::dispatch(services, guild, id, data, user_id).await
        }
        _ if id.action == "addon" || id.action == "update_all" => {
            addons::dispatch(services, guild, id, data, user_id).await
        }
        "cfg" | "cfgmodal" => modules::dispatch(services, guild, id, data, user_id).await,
        _ => Ok(HubOutcome::Done),
    }
}

/// Whether the interacting member holds the `admin.*` permit.
pub async fn caller_may_manage(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> bool {
    let Some(guild_id) = component.guild_id else {
        return false;
    };
    if services.cfg.is_owner(component.user.id.get()) {
        return true;
    }
    let Some(guild) = ctx.cache.guild(guild_id) else {
        return false;
    };
    let Some(member) = guild.members.get(&component.user.id) else {
        return false;
    };
    let perms = guild.member_permissions(member);
    let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let check = CheckContext {
        guild_id: guild_id.get().to_string(),
        user_id: component.user.id.get(),
        channel_id: Some(component.channel_id.get()),
        role_ids: &roles,
        is_owner: false,
        is_guild_owner: guild.owner_id == component.user.id,
        is_manager: perms.administrator() || perms.manage_guild(),
        is_quarantined: false,
        node: ADMIN_NODE,
    };
    services.permits.resolve(&check).allowed
}

async fn caller_may_manage_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
) -> bool {
    if services.cfg.is_owner(modal.user.id.get()) {
        return true;
    }
    let Some(guild_id) = modal.guild_id else {
        return false;
    };
    let Some(guild) = ctx.cache.guild(guild_id) else {
        return false;
    };
    let Some(member) = guild.members.get(&modal.user.id) else {
        return false;
    };
    let perms = guild.member_permissions(member);
    let roles: Vec<u64> = member.roles.iter().map(|r| r.get()).collect();
    let check = CheckContext {
        guild_id: guild_id.get().to_string(),
        user_id: modal.user.id.get(),
        channel_id: Some(modal.channel_id.get()),
        role_ids: &roles,
        is_owner: false,
        is_guild_owner: guild.owner_id == modal.user.id,
        is_manager: perms.administrator() || perms.manage_guild(),
        is_quarantined: false,
        node: ADMIN_NODE,
    };
    services.permits.resolve(&check).allowed
}

async fn respond_outcome(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
    outcome: HubOutcome,
) -> Result<(), Error> {
    match outcome {
        HubOutcome::Update(card) => {
            component
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(v2_message(card.components)),
                )
                .await?;
        }
        HubOutcome::Ephemeral(card) => {
            component
                .create_followup(
                    &ctx.http,
                    serenity::CreateInteractionResponseFollowup::new()
                        .components(card.components)
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .ephemeral(true),
                )
                .await?;
        }
        HubOutcome::Modal(modal) => {
            component
                .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
                .await?;
        }
        HubOutcome::Done => {}
    }
    Ok(())
}

async fn respond_modal(
    ctx: &serenity::Context,
    modal: &ModalInteraction,
    outcome: HubOutcome,
) -> Result<(), Error> {
    match outcome {
        HubOutcome::Update(card) => {
            modal
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::UpdateMessage(v2_message(card.components)),
                )
                .await?;
        }
        HubOutcome::Ephemeral(card) => {
            modal
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Message(
                        v2_message(card.components).flags(MessageFlags::EPHEMERAL),
                    ),
                )
                .await?;
        }
        HubOutcome::Modal(payload) => {
            modal
                .create_response(&ctx.http, CreateInteractionResponse::Modal(payload))
                .await?;
        }
        HubOutcome::Done => {
            modal
                .create_response(
                    &ctx.http,
                    CreateInteractionResponse::Defer(
                        serenity::CreateInteractionResponseMessage::new(),
                    ),
                )
                .await?;
        }
    }
    Ok(())
}

async fn acknowledge(
    ctx: &serenity::Context,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    component
        .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
        .await?;
    Ok(())
}

async fn deny(ctx: &serenity::Context, component: &ComponentInteraction) -> Result<(), Error> {
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(format!("You need the `{ADMIN_NODE}` permit to manage this server."))
                    .flags(MessageFlags::EPHEMERAL),
            ),
        )
        .await?;
    Ok(())
}

async fn deny_modal(ctx: &serenity::Context, modal: &ModalInteraction) -> Result<(), Error> {
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content("You are not allowed to use this panel.")
                    .flags(MessageFlags::EPHEMERAL),
            ),
        )
        .await?;
    Ok(())
}

/// Read a labeled input value out of a modal submission.
pub fn read_input(components: &[serenity::ModalComponent], custom_id: &str) -> Option<String> {
    for component in components {
        if let serenity::ModalComponent::Label(label) = component {
            if let serenity::LabelComponent::InputText(input) = &label.component {
                if input.custom_id == custom_id {
                    return Some(input.value.to_string());
                }
            }
        }
    }
    None
}

/// Wrap V2 components in an update message payload.
pub fn v2_message(
    components: Vec<serenity::CreateComponent<'static>>,
) -> CreateInteractionResponseMessage<'static> {
    CreateInteractionResponseMessage::new()
        .components(components)
        .flags(MessageFlags::IS_COMPONENTS_V2)
        .allowed_mentions(CreateAllowedMentions::new())
}

/// Guild display context from the cache, falling back to a placeholder.
pub fn guild_ctx(ctx: &serenity::Context, guild_id: GuildId) -> GuildCtx {
    ctx.cache.guild(guild_id).map_or(
        GuildCtx {
            id: guild_id.get(),
            name: "this server".to_string(),
            icon: None,
        },
        |guild| GuildCtx {
            id: guild_id.get(),
            name: guild.name.to_string(),
            icon: guild.icon_url(),
        },
    )
}

/// A tab button, marked active when `view == active`.
pub fn tab_button(
    view: HubView,
    active: HubView,
    label: &str,
    emoji: char,
) -> serenity::CreateButton<'static> {
    let style = if view == active {
        serenity::ButtonStyle::Primary
    } else {
        serenity::ButtonStyle::Secondary
    };
    serenity::CreateButton::new(cid(&["tab", view.as_str()]))
        .label(label.to_string())
        .emoji(emoji)
        .style(style)
}

/// The five-tab row, marking `active`.
pub fn tab_row(active: HubView) -> serenity::CreateActionRow<'static> {
    lumi_ui::panels::action_row_of(vec![
        tab_button(HubView::Home, active, "Hub", '🤖'),
        tab_button(HubView::Modules, active, "Modules", '⚙'),
        tab_button(HubView::Permissions, active, "Permissions", '🛡'),
        tab_button(HubView::Settings, active, "Settings", '🏰'),
        tab_button(HubView::Addons, active, "Addons", '📦'),
    ])
}
