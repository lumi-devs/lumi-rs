use poise::serenity_prelude as serenity;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};

async fn autocomplete_repo<'a>(
    ctx: Context<'a>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    let downloader = &ctx.data().downloader;
    let choices: Vec<serenity::AutocompleteChoice> = if let Ok(repos) = downloader.list_repos().await {
        repos
            .into_iter()
            .map(|r| r.name)
            .filter(|name| name.to_lowercase().starts_with(&partial.to_lowercase()))
            .map(|name| serenity::AutocompleteChoice::new(name.clone(), name))
            .collect()
    } else {
        Vec::new()
    };

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    subcommands("repo_add", "repo_list", "repo_remove"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn repo(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "Repository Management",
        "Use `/repo add`, `/repo list`, or `/repo remove` to manage addon repositories.",
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, rename = "add", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn repo_add(
    ctx: Context<'_>,
    #[description = "Git clone URL"] url: String,
    #[description = "Repository name (optional)"] name: Option<String>,
    #[description = "Git branch (optional)"] branch: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.add_repo(&url, name.as_deref(), branch.as_deref()).await {
        Ok(repo) => {
            let lines = vec![
                format!("**Name**: `{}`", repo.name),
                format!("**URL**: `{}`", repo.url),
                format!("**Branch**: `{}`", repo.branch),
                format!("**Commit**: `{}`", repo.commit.as_deref().unwrap_or("unknown")),
            ];
            let card = Card::lines("📦 Repository Added", lines).kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(err) => {
            let card = Card::error("Failed to Add Repository", err.to_string());
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

#[poise::command(slash_command, prefix_command, rename = "list", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn repo_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.list_repos().await {
        Ok(repos) => {
            if repos.is_empty() {
                let card = Card::info("Repositories", "No repositories added yet. Use `/repo add <url>`.");
                ctx.send(card.into_reply().reply()).await?;
            } else {
                let lines: Vec<String> = repos
                    .iter()
                    .map(|r| {
                        let commit_str = r.commit.as_ref().map_or("none", |c| {
                            if c.len() > 7 { &c[..7] } else { c }
                        });
                        format!("• **{}** (`{}`) — branch `{}` (`{}`)", r.name, r.url, r.branch, commit_str)
                    })
                    .collect();
                let card = Card::lines("📦 Added Repositories", lines).kind(CardKind::Info);
                ctx.send(card.into_reply().reply()).await?;
            }
        }
        Err(err) => {
            let card = Card::error("Failed to List Repositories", err.to_string());
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

#[poise::command(slash_command, prefix_command, rename = "remove", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn repo_remove(
    ctx: Context<'_>,
    #[description = "Repository name to remove"]
    #[autocomplete = "autocomplete_repo"]
    name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.remove_repo(&name).await {
        Ok(true) => {
            let card = Card::success("Repository Removed", format!("Removed repository **{name}**."));
            ctx.send(card.into_reply().reply()).await?;
        }
        Ok(false) => {
            let card = Card::warning("Repository Not Found", format!("Repository **{name}** was not found."));
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(err) => {
            let card = Card::error("Failed to Remove Repository", err.to_string());
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    subcommands("download_install", "download_list"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn download(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "Downloader",
        "Use `/download install <repo> <addon>` or `/download list` to manage installed addons.",
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command, rename = "install", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn download_install(
    ctx: Context<'_>,
    #[description = "Repository name containing the addon"]
    #[autocomplete = "autocomplete_repo"]
    repo: String,
    #[description = "Addon name to install"] addon: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.install_module(&repo, &addon).await {
        Ok(module) => {
            let lines = vec![
                format!("**Addon**: `{}`", module.module_name),
                format!("**Repository**: `{repo}`"),
                format!("**Version**: `{}`", module.version.as_deref().unwrap_or("unknown")),
                format!("**Commit**: `{}`", module.commit.as_deref().unwrap_or("unknown")),
            ];
            let card = Card::lines("📥 Addon Installed", lines).kind(CardKind::Success);
            ctx.send(card.into_reply().reply()).await?;
        }
        Err(err) => {
            let card = Card::error("Failed to Install Addon", err.to_string());
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

#[poise::command(slash_command, prefix_command, rename = "list", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn download_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.list_installed_modules().await {
        Ok(modules) => {
            if modules.is_empty() {
                let card = Card::info("Installed Addons", "No addons installed yet. Use `/download install <repo> <addon>`.");
                ctx.send(card.into_reply().reply()).await?;
            } else {
                let mut lines = Vec::new();
                for m in &modules {
                    let repo_label = match downloader.get_repo_by_id(m.repo_id).await {
                        Ok(Some(r)) => r.name,
                        _ => format!("repo #{}", m.repo_id),
                    };
                    let ver = m.version.as_deref().unwrap_or("unversioned");
                    lines.push(format!("• **{}** (v{ver}) from `{repo_label}`", m.module_name));
                }
                let card = Card::lines("📥 Installed Addons", lines).kind(CardKind::Info);
                ctx.send(card.into_reply().reply()).await?;
            }
        }
        Err(err) => {
            let card = Card::error("Failed to List Installed Addons", err.to_string());
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![repo(), download()]
}
