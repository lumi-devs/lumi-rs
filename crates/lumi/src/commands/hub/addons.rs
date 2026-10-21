use std::collections::HashSet;

use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateInputText, CreateLabel, CreateModal,
    CreateModalComponent, InputTextStyle, ModalInteraction,
};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_kernel::downloader::AutoUpdateConfig;
use lumi_ui::panels::{action_row_of, back_button, pagination_row, setting_row, string_select};
use lumi_ui::{Card, CardKind, CardOptions, CardReply, cut_text};

use super::{GuildCtx, HubId, HubOutcome, HubView, cid, read_input, selected_value, tab_row};

/// Selectable auto-update cadences (minutes, label).
const AUTO_UPDATE_INTERVALS: [(u64, &str); 5] = [
    (15, "Every 15 Minutes"),
    (60, "Every Hour"),
    (360, "Every 6 Hours"),
    (720, "Every 12 Hours"),
    (1440, "Every 24 Hours"),
];

/// The add-ons dashboard: repo/installed counts and pending updates.
pub async fn dashboard_card(
    services: &Services,
    guild: &GuildCtx,
    user_id: u64,
) -> Result<CardReply, Error> {
    let repos = services.downloader.list_repos().await?;
    let installed = services.downloader.get_installed_modules_detailed().await?;
    let pending = services
        .downloader
        .check_for_updates_detailed()
        .await
        .unwrap_or_default();

    let mut body = vec![
        "Manage external add-on repositories and installed modules.".to_string(),
        format!("📦 **Tracked Repositories:** {}", repos.len()),
        format!("📥 **Installed Add-ons:** {}", installed.len()),
    ];
    if !pending.is_empty() {
        body.push(format!("⚠️ **Updates available:** {}", pending.join(", ")));
    }
    if !services.cfg.is_owner(user_id) {
        body.push(
            "-# Bot Owner access is required to manage repositories and settings.".to_string(),
        );
    }

    let nav = action_row_of(vec![
        CreateButton::new(cid(&["addon", "repos"]))
            .label("Repositories")
            .emoji('📦')
            .style(ButtonStyle::Primary),
        CreateButton::new(cid(&["addon", "installed"]))
            .label("Installed")
            .emoji('📥')
            .style(ButtonStyle::Primary),
        CreateButton::new(cid(&["addon", "autoupdate"]))
            .label("Auto-update")
            .emoji('⏱')
            .style(ButtonStyle::Secondary),
        CreateButton::new(cid(&["addon", "add_repo"]))
            .label("Add Repo")
            .emoji('➕')
            .style(ButtonStyle::Success),
    ]);

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Addons".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![nav, tab_row(HubView::Addons)];

    Ok(Card::lines("📦 Add-ons", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Configured repositories with per-repo update buttons and a browse select.
async fn repos_card(services: &Services) -> Result<CardReply, Error> {
    let repos = services.downloader.list_repos().await?;

    let mut sections = Vec::new();
    for repo in &repos {
        let sha = repo.commit.as_deref().map_or_else(
            || "unknown".to_string(),
            |c| c.chars().take(7).collect::<String>(),
        );
        let update = CreateButton::new(cid(&["addon", "update_repo", repo.name.as_str()]))
            .label("Update")
            .style(ButtonStyle::Secondary);
        sections.push(setting_row(
            format!("📦 **{}** branch `{}` @ `{}`", repo.name, repo.branch, sha),
            update,
        ));
    }

    let mut rows = vec![action_row_of(vec![
        CreateButton::new(cid(&["addon", "add_repo"]))
            .label("Add Repo")
            .emoji('📦')
            .style(ButtonStyle::Success),
        CreateButton::new(cid(&["addon", "rm_repo"]))
            .label("Remove Repo")
            .emoji('🗑')
            .style(ButtonStyle::Danger),
    ])];
    if !repos.is_empty() {
        let options = repos
            .iter()
            .take(25)
            .map(|r| (r.name.clone(), r.name.clone()))
            .collect();
        rows.push(string_select(
            cid(&["addon", "repo_pick"]),
            "📦 Browse a repository…",
            options,
        ));
    }
    rows.push(back_button(cid(&["tab", "addons"])));
    rows.push(tab_row(HubView::Addons));

    let body = if repos.is_empty() {
        vec!["-# *No repositories added yet. Click Add Repo to get started.*".to_string()]
    } else {
        Vec::new()
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        "Addons".to_string(),
        "Repositories".to_string(),
    ];
    opts.separator_above_action_rows = true;
    opts.sections = sections;
    opts.action_rows = rows;
    opts.footer = Some("Update pulls the repository's latest commit.".to_string());

    Ok(Card::lines("📦 Configure Repositories", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Installed modules with uninstall accessories and per-module toggle buttons.
async fn installed_card(services: &Services) -> Result<CardReply, Error> {
    let installed = services.downloader.get_installed_modules_detailed().await?;

    let mut sections = Vec::new();
    for module in &installed {
        let uninstall = CreateButton::new(cid(&[
            "addon",
            "modact",
            "uninstall",
            module.repo_name.as_str(),
            module.module_name.as_str(),
        ]))
        .label("Uninstall")
        .style(ButtonStyle::Secondary);
        sections.push(setting_row(
            format!(
                "🧩 **{}** `v{}` from **{}**",
                module.module_name,
                module.version.as_deref().unwrap_or("1.0.0"),
                module.repo_name
            ),
            uninstall,
        ));
    }

    let mut rows: Vec<CreateActionRow<'static>> = Vec::new();
    // why: at most 3 toggle rows so toggle + back + tabs stay within Discord's 5-row limit.
    let toggles: Vec<_> = installed.iter().take(15).collect();
    for chunk in toggles.chunks(5) {
        let buttons = chunk
            .iter()
            .map(|m| {
                CreateButton::new(cid(&["addon", "toggle", m.module_name.as_str()]))
                    .label(m.module_name.clone())
                    .emoji('🔁')
                    .style(ButtonStyle::Secondary)
            })
            .collect();
        rows.push(action_row_of(buttons));
    }
    rows.push(back_button(cid(&["tab", "addons"])));
    rows.push(tab_row(HubView::Addons));

    let body = if installed.is_empty() {
        vec!["-# *No add-on modules are currently installed.*".to_string()]
    } else {
        Vec::new()
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        "Addons".to_string(),
        "Installed".to_string(),
    ];
    opts.separator_above_action_rows = true;
    opts.sections = sections;
    opts.action_rows = rows;
    opts.footer = Some("Toggling a module applies instantly.".to_string());

    Ok(Card::lines("📥 Configure Addons", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

async fn autoupdate_card(services: &Services) -> Result<CardReply, Error> {
    let cfg = services.downloader.get_auto_update_config().await?;

    let toggle = CreateButton::new(cid(&["addon", "autoupdate_toggle"]))
        .label(if cfg.enabled {
            "Disable Auto-Update"
        } else {
            "Enable Auto-Update"
        })
        .emoji(if cfg.enabled { '❌' } else { '✅' })
        .style(if cfg.enabled {
            ButtonStyle::Danger
        } else {
            ButtonStyle::Success
        });

    let options = AUTO_UPDATE_INTERVALS
        .iter()
        .map(|(minutes, label)| ((*label).to_string(), minutes.to_string()))
        .collect();
    let interval = string_select(
        cid(&["addon", "autoupdate_interval"]),
        "⏱ Check interval…",
        options,
    );

    let body = vec![
        "When enabled, Lumi periodically checks every tracked repository and pulls updates for installed add-ons automatically.".to_string(),
        format!(
            "**Status:** {}",
            if cfg.enabled {
                "🟢 `ENABLED`"
            } else {
                "🔴 `DISABLED`"
            }
        ),
        format!("**Interval:** every {} minutes", cfg.interval_minutes),
    ];

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        "Addons".to_string(),
        "Auto-update".to_string(),
    ];
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![
        action_row_of(vec![toggle]),
        interval,
        back_button(cid(&["tab", "addons"])),
        tab_row(HubView::Addons),
    ];

    Ok(Card::lines("⏱ Auto-Update Settings", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// One page of a repository's modules, with install/uninstall select and info buttons.
async fn repo_modules_card(
    services: &Services,
    repo_name: &str,
    page: usize,
) -> Result<CardReply, Error> {
    const PER_PAGE: usize = 25;

    let modules: Vec<_> = services
        .downloader
        .get_modules_in_repo(repo_name)
        .await?
        .into_iter()
        .filter(|m| !m.hidden)
        .collect();
    let installed: HashSet<String> = services
        .downloader
        .list_installed_modules()
        .await?
        .into_iter()
        .map(|m| m.module_name)
        .collect();

    let total_pages = modules.len().div_ceil(PER_PAGE).max(1);
    let safe_page = page.min(total_pages - 1);
    let start = safe_page.saturating_mul(PER_PAGE).min(modules.len());
    let end = start.saturating_add(PER_PAGE).min(modules.len());
    let shown = &modules[start..end];

    let mut body = Vec::new();
    let mut select_options = Vec::new();
    for module in shown {
        let is_installed = installed.contains(&module.name);
        let suffix = if is_installed { " ✅ installed" } else { "" };
        body.push(format!(
            "🧩 **{}** `v{}`{}",
            module.name, module.version, suffix
        ));
        let desc = module
            .short
            .clone()
            .or_else(|| module.description.clone())
            .unwrap_or_default();
        if !desc.is_empty() {
            body.push(format!("-# {}", cut_text(&desc, 90)));
        }
        let act = if is_installed { "uninstall" } else { "install" };
        select_options.push((
            format!("{act}: {}", module.name),
            format!("{act}:{repo_name}:{}", module.name),
        ));
    }
    if modules.is_empty() {
        body.push("-# *No modules found in this repository.*".to_string());
    }

    let mut rows: Vec<CreateActionRow<'static>> = Vec::new();
    if !select_options.is_empty() {
        rows.push(string_select(
            cid(&["addon", "mod_action"]),
            "Install or uninstall a module…",
            select_options,
        ));
        let info: Vec<CreateButton<'static>> = shown
            .iter()
            .take(5)
            .map(|m| {
                CreateButton::new(cid(&["addon", "modinfo", repo_name, m.name.as_str()]))
                    .label(m.name.clone())
                    .emoji('📄')
                    .style(ButtonStyle::Secondary)
            })
            .collect();
        rows.push(action_row_of(info));
    }
    rows.push(back_button(cid(&["addon", "installed"])));
    if let Some(pager) = pagination_row(
        &format!("lumi:addon:browsepage:{repo_name}:dir"),
        safe_page,
        total_pages,
    ) {
        rows.push(pager);
    }
    rows.push(tab_row(HubView::Addons));

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Hub".to_string(),
        "Addons".to_string(),
        "Modules".to_string(),
        repo_name.to_string(),
    ];
    opts.separator_above_action_rows = true;
    opts.action_rows = rows;

    Ok(Card::lines(format!("⚙ Modules in {repo_name}"), body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Ephemeral detail card for a single module manifest.
async fn module_info_card(
    services: &Services,
    repo_name: &str,
    module_name: &str,
) -> Result<CardReply, Error> {
    let modules = services.downloader.get_modules_in_repo(repo_name).await?;
    let Some(module) = modules.into_iter().find(|m| m.name == module_name) else {
        return Ok(Card::error(
            "Not Found",
            format!("Module **{module_name}** was not found in **{repo_name}**."),
        )
        .ephemeral()
        .into_reply());
    };

    let lines = [
        format!("### 📦 __{}__ (v{})", module.name, module.version),
        module.description.clone().map_or_else(
            || "*No description provided.*".to_string(),
            |d| format!("*{d}*"),
        ),
        String::new(),
        format!(
            "**Author:** {}",
            module.author.as_deref().unwrap_or("Unknown")
        ),
        format!(
            "**Min Bot Version:** `{}`",
            module.min_bot_version.as_deref().unwrap_or("Any")
        ),
        String::new(),
        "### 🛡 __Data & Privacy Statement__".to_string(),
        module.end_user_data_statement.clone().unwrap_or_else(|| {
            "*This module has not specified an end-user data statement.*".to_string()
        }),
    ];

    Ok(
        Card::info(format!("Module Info: {}", module.name), lines.join("\n"))
            .ephemeral()
            .into_reply(),
    )
}

fn update_confirm_card(repo_name: &str, changelog: Option<&str>) -> CardReply {
    let body = changelog.filter(|c| !c.is_empty()).map_or_else(
        || "New commits are available on the tracked branch.".to_string(),
        |c| format!("```\n{}\n```", cut_text(c, 820)),
    );

    let confirm = CreateButton::new(cid(&["addon", "update_repo_confirm", repo_name]))
        .label("Update")
        .emoji('📥')
        .style(ButtonStyle::Success);
    let skip = CreateButton::new(cid(&["addon", "update_repo_skip", repo_name]))
        .label("Skip")
        .style(ButtonStyle::Secondary);

    let mut opts = CardOptions::default();
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![action_row_of(vec![confirm, skip]), tab_row(HubView::Addons)];

    Card::new(format!("📦 Update available for {repo_name}"), body)
        .kind(CardKind::Warning)
        .options(opts)
        .into_reply()
}

/// Pull every repository, returning an ephemeral summary.
async fn update_all(services: &Services) -> Result<HubOutcome, Error> {
    let repos = services.downloader.list_repos().await?;
    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for repo in repos {
        match services.downloader.update_repo(&repo.name).await {
            Ok(_) => updated.push(repo.name),
            Err(e) => failed.push(format!("**{}** — {e}", repo.name)),
        }
    }

    let reply = if failed.is_empty() {
        Card::success(
            "Add-ons Updated",
            format!(
                "Updated {} repositor{}.",
                updated.len(),
                if updated.len() == 1 { "y" } else { "ies" }
            ),
        )
        .into_reply()
    } else {
        let mut body = vec![if updated.is_empty() {
            "Nothing updated.".to_string()
        } else {
            format!("Updated: {}", updated.join(", "))
        }];
        body.extend(failed);
        Card::error("Some Add-ons Failed To Update", body.join("\n")).into_reply()
    };
    Ok(HubOutcome::Ephemeral(reply))
}

pub async fn dispatch(
    services: &Services,
    _guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    // why: mod.rs only enforces `admin.*` for buttons, so owner-gate management here.
    if !services.cfg.is_owner(user_id) {
        return Ok(HubOutcome::Ephemeral(
            Card::error("Access Denied", "Only Bot Owners can manage add-ons.").into_reply(),
        ));
    }

    match (id.head().unwrap_or_default(), id.get(1)) {
        ("update_all", _) => update_all(services).await,
        ("addon", Some("repos")) => Ok(HubOutcome::Update(repos_card(services).await?)),
        ("addon", Some("installed")) => Ok(HubOutcome::Update(installed_card(services).await?)),
        ("addon", Some("autoupdate")) => Ok(HubOutcome::Update(autoupdate_card(services).await?)),
        ("addon", Some("autoupdate_toggle")) => {
            let cfg = services.downloader.get_auto_update_config().await?;
            services
                .downloader
                .set_auto_update_config(AutoUpdateConfig {
                    enabled: !cfg.enabled,
                    interval_minutes: cfg.interval_minutes,
                })
                .await?;
            Ok(HubOutcome::Update(autoupdate_card(services).await?))
        }
        ("addon", Some("autoupdate_interval")) => {
            if let Some(minutes) = selected_value(data).and_then(|v| v.parse::<u64>().ok())
                && minutes > 0
            {
                let cfg = services.downloader.get_auto_update_config().await?;
                services
                    .downloader
                    .set_auto_update_config(AutoUpdateConfig {
                        enabled: cfg.enabled,
                        interval_minutes: minutes,
                    })
                    .await?;
            }
            Ok(HubOutcome::Update(autoupdate_card(services).await?))
        }
        ("addon", Some("toggle")) => {
            let module = id.tail(2);
            if module.is_empty() {
                return Ok(HubOutcome::Done);
            }
            let current = services
                .downloader
                .is_module_enabled(&module)
                .await
                .unwrap_or(true);
            services.downloader.toggle_module(&module, !current).await?;
            Ok(HubOutcome::Update(installed_card(services).await?))
        }
        ("addon", Some("update_repo")) => {
            let repo = id.tail(2);
            if repo.is_empty() {
                return Ok(HubOutcome::Done);
            }
            let check = services.downloader.check_repo_update(&repo).await?;
            if !check.ok {
                return Ok(HubOutcome::Ephemeral(
                    Card::error("Check Failed", check.reason).into_reply(),
                ));
            }
            if !check.has_update {
                return Ok(HubOutcome::Ephemeral(
                    Card::info(
                        "Already Up To Date",
                        format!("**{repo}** has no pending updates."),
                    )
                    .into_reply(),
                ));
            }
            Ok(HubOutcome::Update(update_confirm_card(
                &repo,
                check.changelog.as_deref(),
            )))
        }
        ("addon", Some("update_repo_confirm")) => {
            let repo = id.tail(2);
            if repo.is_empty() {
                return Ok(HubOutcome::Done);
            }
            // shortcut: HubOutcome carries one reply, so the pull reports ephemerally
            // instead of also re-rendering the repos card; reopen to see the new sha.
            let reply = match services.downloader.update_repo(&repo).await {
                Ok(result) => {
                    let sha_line = if result.changed {
                        format!(
                            "`{}` → `{}`",
                            result
                                .old_sha
                                .as_deref()
                                .unwrap_or("?")
                                .chars()
                                .take(7)
                                .collect::<String>(),
                            result.new_sha.chars().take(7).collect::<String>()
                        )
                    } else {
                        "Already up to date.".to_string()
                    };
                    let mut body = vec![format!("**{repo}** was updated."), sha_line];
                    if let Some(stat) = result.diff_stat.filter(|s| !s.is_empty()) {
                        body.push(format!("```diff\n{}\n```", cut_text(&stat, 900)));
                    }
                    Card::success("Repository Updated", body.join("\n\n")).into_reply()
                }
                Err(e) => Card::error("Update Failed", e.to_string()).into_reply(),
            };
            Ok(HubOutcome::Ephemeral(reply))
        }
        ("addon", Some("update_repo_skip")) => Ok(HubOutcome::Update(repos_card(services).await?)),
        ("addon", Some("browsepage")) => {
            let repo = id.get(2).unwrap_or_default().to_string();
            let page = id.get(4).and_then(|p| p.parse::<usize>().ok()).unwrap_or(0);
            if repo.is_empty() {
                return Ok(HubOutcome::Done);
            }
            Ok(HubOutcome::Update(
                repo_modules_card(services, &repo, page).await?,
            ))
        }
        ("addon", Some("modact")) => {
            let act = id.get(2).unwrap_or_default();
            let repo = id.get(3).unwrap_or_default();
            let module = id.tail(4);
            if act.is_empty() || repo.is_empty() || module.is_empty() {
                return Ok(HubOutcome::Done);
            }
            if let Err(e) = apply_module_action(services, act, repo, &module).await {
                return Ok(HubOutcome::Ephemeral(
                    Card::error("Action Failed", e.to_string()).into_reply(),
                ));
            }
            Ok(HubOutcome::Update(
                repo_modules_card(services, repo, 0).await?,
            ))
        }
        ("addon", Some("modinfo")) => {
            let repo = id.get(2).unwrap_or_default().to_string();
            let module = id.tail(3);
            if repo.is_empty() || module.is_empty() {
                return Ok(HubOutcome::Done);
            }
            Ok(HubOutcome::Ephemeral(
                module_info_card(services, &repo, &module).await?,
            ))
        }
        ("addon", Some("repo_pick")) => {
            let repo = selected_value(data).unwrap_or_default();
            if repo.is_empty() {
                return Ok(HubOutcome::Done);
            }
            Ok(HubOutcome::Update(
                repo_modules_card(services, &repo, 0).await?,
            ))
        }
        ("addon", Some("mod_action")) => {
            let value = selected_value(data).unwrap_or_default();
            let Some((act, rest)) = value.split_once(':') else {
                return Ok(HubOutcome::Done);
            };
            let Some((repo, module)) = rest.split_once(':') else {
                return Ok(HubOutcome::Done);
            };
            if act.is_empty() || repo.is_empty() || module.is_empty() {
                return Ok(HubOutcome::Done);
            }
            if let Err(e) = apply_module_action(services, act, repo, module).await {
                return Ok(HubOutcome::Ephemeral(
                    Card::error("Action Failed", e.to_string()).into_reply(),
                ));
            }
            Ok(HubOutcome::Update(
                repo_modules_card(services, repo, 0).await?,
            ))
        }
        ("addon", Some("add_repo")) => Ok(HubOutcome::Modal(add_repo_modal())),
        ("addon", Some("rm_repo")) => Ok(HubOutcome::Modal(rm_repo_modal())),
        ("addon", Some("install")) => Ok(HubOutcome::Modal(install_modal())),
        ("addon", Some("uninstall")) => Ok(HubOutcome::Modal(uninstall_modal())),
        _ => Ok(HubOutcome::Done),
    }
}

async fn apply_module_action(
    services: &Services,
    act: &str,
    repo: &str,
    module: &str,
) -> Result<(), Error> {
    match act {
        "install" => services
            .downloader
            .install_module(repo, module)
            .await
            .map(|_| ())
            .map_err(Into::into),
        "uninstall" => services
            .downloader
            .uninstall_module(module)
            .await
            .map(|_| ()),
        _ => Ok(()),
    }
}

pub async fn handle_modal(
    services: &Services,
    _guild: &GuildCtx,
    modal: &ModalInteraction,
    action: &str,
) -> Result<HubOutcome, Error> {
    let outcome: Result<(String, String), Error> = match action {
        "add_repo" => {
            let url = read_input(&modal.data.components, "url").unwrap_or_default();
            let url = url.trim().to_string();
            if url.is_empty() {
                return Ok(HubOutcome::Ephemeral(
                    Card::error("Addon Error", "A repository URL is required.").into_reply(),
                ));
            }
            let name = read_input(&modal.data.components, "name").unwrap_or_default();
            let branch = read_input(&modal.data.components, "branch").unwrap_or_default();
            services
                .downloader
                .add_repo(&url, optional(&name), optional(&branch))
                .await
                .map(|repo| {
                    (
                        "Repository Added".to_string(),
                        format!("**{}** was cloned and added.", repo.name),
                    )
                })
                .map_err(Into::into)
        }
        "rm_repo" => {
            let name = read_input(&modal.data.components, "name")
                .unwrap_or_default()
                .trim()
                .to_string();
            services
                .downloader
                .remove_repo(&name)
                .await
                .map(|_| {
                    (
                        "Repository Removed".to_string(),
                        format!("Removed **{name}** and any modules installed from it."),
                    )
                })
                .map_err(Into::into)
        }
        "install" => {
            let repo = read_input(&modal.data.components, "repo")
                .unwrap_or_default()
                .trim()
                .to_string();
            let module = read_input(&modal.data.components, "module")
                .unwrap_or_default()
                .trim()
                .to_string();
            services
                .downloader
                .install_module(&repo, &module)
                .await
                .map(|_| {
                    (
                        "Module Installed".to_string(),
                        format!("Installed **{module}** from **{repo}**."),
                    )
                })
                .map_err(Into::into)
        }
        "uninstall" => {
            let module = read_input(&modal.data.components, "module")
                .unwrap_or_default()
                .trim()
                .to_string();
            services
                .downloader
                .uninstall_module(&module)
                .await
                .map(|found| {
                    (
                        "Module Uninstalled".to_string(),
                        if found {
                            format!("Uninstalled **{module}**.")
                        } else {
                            format!("Module **{module}** was not installed via the downloader.")
                        },
                    )
                })
        }
        _ => return Ok(HubOutcome::Done),
    };

    match outcome {
        Ok((title, body)) => Ok(HubOutcome::Ephemeral(
            Card::success(title, body).into_reply(),
        )),
        Err(e) => Ok(HubOutcome::Ephemeral(
            Card::error("Addon Error", e.to_string()).into_reply(),
        )),
    }
}

fn add_repo_modal() -> CreateModal<'static> {
    CreateModal::new(cid(&["addonmodal", "add_repo"]), "Add Repository").components(vec![
        text_input(
            "url",
            "Repository URL",
            "e.g. https://github.com/owner/repo",
            true,
        ),
        text_input(
            "name",
            "Repository Name (optional)",
            "Leave blank to derive from the URL",
            false,
        ),
        text_input(
            "branch",
            "Branch (optional)",
            "default: repository default",
            false,
        ),
    ])
}

fn rm_repo_modal() -> CreateModal<'static> {
    CreateModal::new(cid(&["addonmodal", "rm_repo"]), "Remove Repository").components(vec![
        text_input("name", "Repository Name", "e.g. lumi-addons", true),
    ])
}

fn install_modal() -> CreateModal<'static> {
    CreateModal::new(cid(&["addonmodal", "install"]), "Install Module").components(vec![
        text_input("repo", "Repository Name", "e.g. lumi-addons", true),
        text_input("module", "Module Name", "e.g. activity-roles", true),
    ])
}

fn uninstall_modal() -> CreateModal<'static> {
    CreateModal::new(cid(&["addonmodal", "uninstall"]), "Uninstall Module").components(vec![
        text_input("module", "Module Name", "e.g. activity-roles", true),
    ])
}

fn text_input(
    custom_id: impl Into<String>,
    label: impl Into<String>,
    placeholder: impl Into<String>,
    required: bool,
) -> CreateModalComponent<'static> {
    CreateModalComponent::Label(CreateLabel::input_text(
        label.into(),
        CreateInputText::new(InputTextStyle::Short, custom_id.into())
            .required(required)
            .placeholder(placeholder.into()),
    ))
}

/// `Some(trimmed)` unless the value is blank.
fn optional(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
