use poise::serenity_prelude as serenity;
use serenity::{
    CreateAllowedMentions, CreateInteractionResponse, CreateInteractionResponseMessage,
    MessageFlags,
};
use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateInputText, CreateLabel, CreateModal,
    CreateModalComponent, InputTextStyle,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_kernel::Services;
use lumi_ui::panels::action_row_of;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

/// Custom-id prefix for every hub interaction.
const HUB_PREFIX: &str = "lumi";
/// Fixed custom id of the set-prefix modal.
const PREFIX_MODAL_ID: &str = "lumi:prefixmodal";
/// Permits beyond this count render read-only (keeps the card within row limits).
const MAX_INTERACTIVE_PERMITS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HubView {
    Home,
    Modules,
    Permissions,
    Settings,
    Addons,
}

impl HubView {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Modules => "modules",
            Self::Permissions => "permissions",
            Self::Settings => "settings",
            Self::Addons => "addons",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
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

#[derive(Clone, Copy)]
enum OrderDir {
    Up,
    Down,
}

enum HubAction {
    Tab(HubView),
    PrefixSet,
    PrefixReset,
    PermOrder { id: i32, direction: OrderDir },
    PermDelete { id: i32 },
}

struct HubParsed {
    user_id: u64,
    action: HubAction,
}

/// Main administrative hub and server feature overview.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    subcommands("panel")
)]
pub async fn lumi(ctx: Context<'_>) -> Result<(), Error> {
    panel_action(ctx).await
}

/// Open the interactive Lumi control panel.
#[poise::command(slash_command, prefix_command, guild_only, rename = "panel")]
pub async fn panel(ctx: Context<'_>) -> Result<(), Error> {
    panel_action(ctx).await
}

async fn panel_action(ctx: Context<'_>) -> Result<(), Error> {
    let (guild_id, guild_name, guild_icon) = {
        let guild = ctx.guild().ok_or_else(|| anyhow::anyhow!("guild only"))?;
        (guild.id.get(), guild.name.to_string(), guild.icon_url())
    };
    let user_id = ctx.author().id.get();
    let card = render_hub(
        ctx.data(),
        guild_id,
        &guild_name,
        guild_icon,
        HubView::Home,
        user_id,
    )
    .await?;
    respond(ctx, &card).await
}

pub fn commands() -> Vec<Command> {
    vec![lumi()]
}

async fn respond(ctx: Context<'_>, card: &CardReply) -> Result<(), Error> {
    match ctx {
        poise::Context::Application(app) => {
            app.interaction
                .create_response(
                    ctx.http(),
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .components(card.components.clone())
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .allowed_mentions(CreateAllowedMentions::new()),
                    ),
                )
                .await?;
        }
        poise::Context::Prefix(pfx) => {
            pfx.msg
                .channel_id
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new()
                        .components(card.components.clone())
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .allowed_mentions(CreateAllowedMentions::new()),
                )
                .await?;
        }
    }
    Ok(())
}

async fn render_hub(
    services: &Services,
    guild_id: u64,
    guild_name: &str,
    guild_icon: Option<String>,
    view: HubView,
    user_id: u64,
) -> Result<CardReply, Error> {
    let card = match view {
        HubView::Home => home_card(services, guild_id, guild_name, guild_icon, user_id).await?,
        HubView::Modules => modules_card(services, guild_name, guild_icon, user_id),
        HubView::Permissions => {
            permissions_card(services, guild_id, guild_name, guild_icon, user_id).await?
        }
        HubView::Settings => settings_card(services, guild_id, guild_name, guild_icon, user_id).await?,
        HubView::Addons => addons_card(services, guild_name, guild_icon, user_id).await?,
    };
    Ok(card)
}

async fn home_card(
    services: &Services,
    guild_id: u64,
    guild_name: &str,
    guild_icon: Option<String>,
    user_id: u64,
) -> Result<CardReply, Error> {
    let prefix = lumi_db::get_prefix(&services.db, Some(guild_id)).await;
    let module_count = services.registry.len();
    let hints = [
        "⚙️ **Modules** - enable, disable, and configure every feature",
        "🛡️ **Permissions** - permit grants and per-command overrides",
        "🏰 **Settings** - server language and command prefix",
        "📦 **Addons** - extend Lumi with add-on modules",
    ]
    .join("\n");
    let body = vec![
        "Manage everything for this server from one place - no scattered commands to remember."
            .to_string(),
        format!("⚙️ **{module_count}** modules loaded"),
        format!("🏰 Locale `en-US`  •  Prefix `{prefix}`"),
        hints,
    ];

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), guild_name.to_string()];
    opts.thumbnail_url = guild_icon;
    opts.footer = Some("Select a tab below to continue.".to_string());
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Home, user_id)];

    Ok(Card::lines("🤖 Lumi Control Panel", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

fn modules_card(
    services: &Services,
    guild_name: &str,
    guild_icon: Option<String>,
    user_id: u64,
) -> CardReply {
    let mut body: Vec<String> = services
        .registry
        .iter()
        .map(|m| {
            let meta = m.metadata();
            format!(
                "{} **{}** `v{}`\n-# {}",
                meta.emoji, meta.display_name, meta.version, meta.description
            )
        })
        .collect();
    if body.is_empty() {
        body.push("-# *No modules loaded.*".to_string());
    }

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), guild_name.to_string(), "Modules".to_string()];
    opts.thumbnail_url = guild_icon;
    opts.footer = Some("Module toggles are not yet available.".to_string());
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Modules, user_id)];

    Card::lines("⚙️ Loaded Modules", body)
        .kind(CardKind::Info)
        .options(opts)
        .into_reply()
}

async fn permissions_card(
    services: &Services,
    guild_id: u64,
    guild_name: &str,
    guild_icon: Option<String>,
    user_id: u64,
) -> Result<CardReply, Error> {
    let permits = lumi_db::list_permits(&services.db, guild_id).await?;
    let mut body = Vec::new();
    let mut rows: Vec<CreateActionRow<'static>> = Vec::new();

    if permits.is_empty() {
        body.push("-# *No permits configured for this server.*".to_string());
    } else {
        for (i, permit) in permits.iter().enumerate() {
            body.push(format!(
                "**{}.** `{}` — {} / {}",
                i + 1,
                permit.name,
                format!("{:?}", permit.kind).to_lowercase(),
                format!("{:?}", permit.polarity).to_lowercase()
            ));
        }
        for permit in permits.iter().take(MAX_INTERACTIVE_PERMITS) {
            rows.push(permit_row(permit.id, user_id));
        }
        if permits.len() > MAX_INTERACTIVE_PERMITS {
            body.push(format!(
                "-# ...and {} more permit(s) are read-only here.",
                permits.len() - MAX_INTERACTIVE_PERMITS
            ));
        }
    }
    rows.push(tab_row(HubView::Permissions, user_id));

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        guild_name.to_string(),
        "Permissions".to_string(),
    ];
    opts.thumbnail_url = guild_icon;
    opts.separator_above_action_rows = true;
    opts.action_rows = rows;

    Ok(Card::lines("🛡️ Permits", body)
        .kind(CardKind::Info)
        .options(opts)
        .into_reply())
}

async fn settings_card(
    services: &Services,
    guild_id: u64,
    guild_name: &str,
    guild_icon: Option<String>,
    user_id: u64,
) -> Result<CardReply, Error> {
    let prefix = lumi_db::get_prefix(&services.db, Some(guild_id)).await;
    let body = vec![
        format!("💻 **Prefix** - `{prefix}`"),
        "-# Prefix commands work for a curated set of moderation and utility commands.".to_string(),
    ];

    let set = CreateButton::new(format!("lumi:prefix:set:{user_id}"))
        .label("Set Prefix")
        .emoji('✏')
        .style(ButtonStyle::Primary);
    let reset = CreateButton::new(format!("lumi:prefix:reset:{user_id}"))
        .label("Reset Prefix")
        .emoji('🗑')
        .style(ButtonStyle::Secondary);

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        guild_name.to_string(),
        "Settings".to_string(),
    ];
    opts.thumbnail_url = guild_icon;
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![
        action_row_of(vec![set]),
        action_row_of(vec![reset]),
        tab_row(HubView::Settings, user_id),
    ];

    Ok(Card::lines("🏰 Server Settings", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

async fn addons_card(
    services: &Services,
    guild_name: &str,
    guild_icon: Option<String>,
    user_id: u64,
) -> Result<CardReply, Error> {
    let repos = services
        .downloader
        .list_repos()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut body = Vec::new();
    if repos.is_empty() {
        body.push("-# *No add-on repositories are added.*".to_string());
    } else {
        for repo in &repos {
            let commit = repo
                .commit
                .as_deref()
                .map(|c| c.chars().take(7).collect::<String>())
                .unwrap_or_else(|| "unknown".to_string());
            body.push(format!(
                "📦 **{}** — branch `{}` — `{}`",
                repo.name, repo.branch, commit
            ));
        }
    }

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), guild_name.to_string(), "Addons".to_string()];
    opts.thumbnail_url = guild_icon;
    opts.footer = Some("Interactive install/update is deferred to a future update.".to_string());
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Addons, user_id)];

    Ok(Card::lines("📦 Add-ons", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

fn tab_row(active: HubView, user_id: u64) -> CreateActionRow<'static> {
    const TABS: [(HubView, &str, char); 5] = [
        (HubView::Home, "Hub", '🤖'),
        (HubView::Modules, "Modules", '⚙'),
        (HubView::Permissions, "Permissions", '🛡'),
        (HubView::Settings, "Settings", '🏰'),
        (HubView::Addons, "Addons", '📦'),
    ];
    let buttons = TABS
        .iter()
        .map(|(view, label, emoji)| {
            let style = if *view == active {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            };
            CreateButton::new(format!("lumi:tab:{}:{user_id}", view.as_str()))
                .label(*label)
                .emoji(*emoji)
                .style(style)
        })
        .collect();
    action_row_of(buttons)
}

fn permit_row(permit_id: i32, user_id: u64) -> CreateActionRow<'static> {
    let up = CreateButton::new(format!("lumi:perm:order:{permit_id}:up:{user_id}"))
        .label("Up")
        .emoji('⬆')
        .style(ButtonStyle::Secondary);
    let down = CreateButton::new(format!("lumi:perm:order:{permit_id}:down:{user_id}"))
        .label("Down")
        .emoji('⬇')
        .style(ButtonStyle::Secondary);
    let delete = CreateButton::new(format!("lumi:perm:del:{permit_id}:{user_id}"))
        .label("Delete")
        .emoji('🗑')
        .style(ButtonStyle::Danger);
    action_row_of(vec![up, down, delete])
}

/// Handle a `lumi:*` button press from the hub.
pub async fn handle_hub_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let Some(parsed) = parse_hub_id(&component.data.custom_id) else {
        return Ok(());
    };
    let user_id = parsed.user_id;

    if component.user.id.get() != user_id {
        component
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("This panel belongs to someone else.")
                        .flags(MessageFlags::EPHEMERAL),
                ),
            )
            .await?;
        return Ok(());
    }

    let Some(guild_id) = component.guild_id.map(serenity::GuildId::get) else {
        return Ok(());
    };

    let view = match &parsed.action {
        HubAction::Tab(tab) => *tab,
        HubAction::PermOrder { .. } | HubAction::PermDelete { .. } => HubView::Permissions,
        HubAction::PrefixSet | HubAction::PrefixReset => HubView::Settings,
    };

    match &parsed.action {
        HubAction::PrefixSet => {
            let modal = CreateModal::new(PREFIX_MODAL_ID, "Set Command Prefix").components(vec![
                CreateModalComponent::Label(CreateLabel::input_text(
                    "New prefix",
                    CreateInputText::new(InputTextStyle::Short, "prefix")
                        .max_length(5)
                        .required(true)
                        .placeholder("e.g. ,"),
                )),
            ]);
            component
                .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
                .await?;
            return Ok(());
        }
        HubAction::PermOrder { id, direction } => {
            let direction = match direction {
                OrderDir::Up => "up",
                OrderDir::Down => "down",
            };
            lumi_db::move_permit(&services.db, guild_id, *id, direction).await?;
        }
        HubAction::PermDelete { id } => {
            lumi_db::delete_permit(&services.db, *id).await?;
        }
        HubAction::PrefixReset => {
            lumi_db::reset_prefix(&services.db, guild_id).await?;
        }
        HubAction::Tab(_) => {}
    }

    let (guild_name, guild_icon) = guild_info(ctx, guild_id);
    let card = render_hub(services, guild_id, &guild_name, guild_icon, view, user_id).await?;
    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.components)
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await?;
    Ok(())
}

/// Handle the `lumi:prefixmodal` submission.
pub async fn handle_hub_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &serenity::ModalInteraction,
) -> Result<(), Error> {
    if modal.data.custom_id != PREFIX_MODAL_ID {
        return Ok(());
    }
    let Some(guild_id) = modal.guild_id.map(serenity::GuildId::get) else {
        return Ok(());
    };

    let raw = read_input(&modal.data.components, "prefix").unwrap_or_default();
    let prefix = raw.trim();

    if prefix.is_empty() || prefix.chars().count() > 5 {
        modal
            .create_response(
                &ctx.http,
                CreateInteractionResponse::Message(
                    CreateInteractionResponseMessage::new()
                        .content("Prefix must be between 1 and 5 characters.")
                        .flags(MessageFlags::EPHEMERAL),
                ),
            )
            .await?;
        return Ok(());
    }

    lumi_db::set_prefix(&services.db, guild_id, prefix).await?;

    let (guild_name, guild_icon) = guild_info(ctx, guild_id);
    let card = render_hub(
        services,
        guild_id,
        &guild_name,
        guild_icon,
        HubView::Settings,
        modal.user.id.get(),
    )
    .await?;
    modal
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.components)
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await?;
    Ok(())
}

fn parse_hub_id(custom_id: &str) -> Option<HubParsed> {
    let mut parts = custom_id.split(':');
    if parts.next()? != HUB_PREFIX {
        return None;
    }
    let action = parts.next()?;
    match action {
        "tab" => {
            let tab = HubView::from_id(parts.next()?)?;
            let user_id = parts.next()?.parse().ok()?;
            Some(HubParsed {
                user_id,
                action: HubAction::Tab(tab),
            })
        }
        "prefix" => {
            let sub = parts.next()?;
            let user_id = parts.next()?.parse().ok()?;
            let action = match sub {
                "set" => HubAction::PrefixSet,
                "reset" => HubAction::PrefixReset,
                _ => return None,
            };
            Some(HubParsed { user_id, action })
        }
        "perm" => {
            let sub = parts.next()?;
            match sub {
                "order" => {
                    let id = parts.next()?.parse().ok()?;
                    let direction = match parts.next()? {
                        "up" => OrderDir::Up,
                        "down" => OrderDir::Down,
                        _ => return None,
                    };
                    let user_id = parts.next()?.parse().ok()?;
                    Some(HubParsed {
                        user_id,
                        action: HubAction::PermOrder { id, direction },
                    })
                }
                "del" => {
                    let id = parts.next()?.parse().ok()?;
                    let user_id = parts.next()?.parse().ok()?;
                    Some(HubParsed {
                        user_id,
                        action: HubAction::PermDelete { id },
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn read_input(components: &[serenity::ModalComponent], custom_id: &str) -> Option<String> {
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

fn guild_info(ctx: &serenity::Context, guild_id: u64) -> (String, Option<String>) {
    ctx.cache
        .guild(serenity::GuildId::new(guild_id))
        .map_or_else(
            || ("this server".to_string(), None),
            |guild| (guild.name.to_string(), guild.icon_url()),
        )
}
