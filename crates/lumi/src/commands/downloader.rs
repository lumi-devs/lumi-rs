use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};
use poise::serenity_prelude as serenity;
use std::net::IpAddr;

/// Check whether an IP address belongs to loopback, private, link-local,
/// unspecified, or documentation blocks (IPv4 and IPv6).
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_unspecified()
                || ipv4.octets()[0] == 0
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                // ULA fc00::/7 (covers fc00::/8 and fd00::/8)
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped IPv6 (::ffff:0:0/96)
                || match ipv6.to_ipv4_mapped() {
                    Some(v4) => is_forbidden_ip(IpAddr::V4(v4)),
                    None => false,
                }
        }
    }
}

fn parse_host_port(url_str: &str) -> Option<(&str, u16)> {
    let (rest, default_port) = if let Some(stripped) = url_str.strip_prefix("https://") {
        (stripped, 443)
    } else if let Some(stripped) = url_str.strip_prefix("http://") {
        (stripped, 80)
    } else {
        return None;
    };
    let authority = rest.split(['/', '?', '#']).next()?;
    // Last '@' wins because userinfo may itself contain one.
    let host_port = authority.split('@').next_back()?;
    if host_port.starts_with('[') {
        let end_bracket = host_port.find(']')?;
        let host = &host_port[1..end_bracket];
        let remaining = &host_port[end_bracket + 1..];
        let port = if let Some(p_str) = remaining.strip_prefix(':') {
            p_str.parse::<u16>().ok()?
        } else {
            default_port
        };
        Some((host, port))
    } else if let Some((host, p_str)) = host_port.split_once(':') {
        let port = p_str.parse::<u16>().ok()?;
        Some((host, port))
    } else {
        Some((host_port, default_port))
    }
}

/// Validate clone URLs against SSRF: ensure http(s) scheme and resolve
/// hostnames, rejecting loopback, link-local, and private ranges in IPv4 and IPv6.
async fn validate_clone_url(raw: &str) -> Result<(), &'static str> {
    let (host, port) =
        parse_host_port(raw).ok_or("Invalid or unsupported URL scheme (must be http/https)")?;
    if host.is_empty() {
        return Err("URL contains an empty host");
    }
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
    {
        return Err("Local and internal hosts are forbidden (SSRF protection)");
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_forbidden_ip(ip) {
            return Err(
                "Private, loopback, or internal IP addresses are forbidden (SSRF protection)",
            );
        }
        return Ok(());
    }

    match tokio::net::lookup_host((host, port)).await {
        Ok(addrs) => {
            let mut resolved_any = false;
            for addr in addrs {
                resolved_any = true;
                if is_forbidden_ip(addr.ip()) {
                    return Err(
                        "Target hostname resolves to a forbidden private/internal IP address",
                    );
                }
            }
            if !resolved_any {
                return Err("Failed to resolve target hostname");
            }
            Ok(())
        }
        Err(_) => Err("Failed to resolve target hostname"),
    }
}

async fn autocomplete_repo<'a>(
    ctx: Context<'a>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    let downloader = &ctx.data().downloader;
    let choices: Vec<serenity::AutocompleteChoice> = match downloader.list_repos().await {
        Ok(repos) => repos
            .into_iter()
            .map(|r| r.name)
            .filter(|name| name.to_lowercase().starts_with(&partial.to_lowercase()))
            .map(|name| serenity::AutocompleteChoice::new(name.clone(), name))
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "repository autocomplete query failed");
            Vec::new()
        }
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

#[poise::command(
    slash_command,
    prefix_command,
    rename = "add",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn repo_add(
    ctx: Context<'_>,
    #[description = "Git clone URL"] url: String,
    #[description = "Repository name (optional)"] name: Option<String>,
    #[description = "Git branch (optional)"] branch: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    if let Err(err) = validate_clone_url(&url).await {
        let card = Card::error("Invalid Repository URL", err);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }
    let downloader = &ctx.data().downloader;
    match downloader
        .add_repo(&url, name.as_deref(), branch.as_deref())
        .await
    {
        Ok(repo) => {
            let lines = vec![
                format!("**Name**: `{}`", repo.name),
                format!("**URL**: `{}`", repo.url),
                format!("**Branch**: `{}`", repo.branch),
                format!(
                    "**Commit**: `{}`",
                    repo.commit.as_deref().unwrap_or("unknown")
                ),
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

#[poise::command(
    slash_command,
    prefix_command,
    rename = "list",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn repo_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.list_repos().await {
        Ok(repos) => {
            if repos.is_empty() {
                let card = Card::info(
                    "Repositories",
                    "No repositories added yet. Use `/repo add <url>`.",
                );
                ctx.send(card.into_reply().reply()).await?;
            } else {
                let lines: Vec<String> = repos
                    .iter()
                    .map(|r| {
                        let commit_str = r
                            .commit
                            .as_ref()
                            .map_or("none", |c| c.get(..7).unwrap_or(c.as_str()));
                        format!(
                            "• **{}** (`{}`) — branch `{}` (`{}`)",
                            r.name, r.url, r.branch, commit_str
                        )
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

#[poise::command(
    slash_command,
    prefix_command,
    rename = "remove",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
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
            let card = Card::success(
                "Repository Removed",
                format!("Removed repository **{name}**."),
            );
            ctx.send(card.into_reply().reply()).await?;
        }
        Ok(false) => {
            let card = Card::warning(
                "Repository Not Found",
                format!("Repository **{name}** was not found."),
            );
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

#[poise::command(
    slash_command,
    prefix_command,
    rename = "install",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
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
                format!(
                    "**Version**: `{}`",
                    module.version.as_deref().unwrap_or("unknown")
                ),
                format!(
                    "**Commit**: `{}`",
                    module.commit.as_deref().unwrap_or("unknown")
                ),
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

#[poise::command(
    slash_command,
    prefix_command,
    rename = "list",
    guild_only,
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn download_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let downloader = &ctx.data().downloader;
    match downloader.list_installed_modules().await {
        Ok(modules) => {
            if modules.is_empty() {
                let card = Card::info(
                    "Installed Addons",
                    "No addons installed yet. Use `/download install <repo> <addon>`.",
                );
                ctx.send(card.into_reply().reply()).await?;
            } else {
                let mut lines = Vec::new();
                for m in &modules {
                    let repo_label = match downloader.get_repo_by_id(m.repo_id).await {
                        Ok(Some(r)) => r.name,
                        Ok(None) => {
                            tracing::warn!(
                                repo_id = m.repo_id,
                                "installed addon references a missing repository"
                            );
                            format!("repo #{}", m.repo_id)
                        }
                        Err(err) => {
                            tracing::warn!(%err, repo_id = m.repo_id, "failed to resolve addon repository");
                            format!("repo #{}", m.repo_id)
                        }
                    };
                    let ver = m.version.as_deref().unwrap_or("unversioned");
                    lines.push(format!(
                        "• **{}** (v{ver}) from `{repo_label}`",
                        m.module_name
                    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hosts_and_ports_correctly() {
        assert_eq!(
            parse_host_port("https://github.com/foo/bar.git"),
            Some(("github.com", 443))
        );
        assert_eq!(
            parse_host_port("http://example.com:8080/repo.git"),
            Some(("example.com", 8080))
        );
        assert_eq!(
            parse_host_port("http://[::1]:9090/repo.git"),
            Some(("::1", 9090))
        );
        assert_eq!(
            parse_host_port("http://[fe80::1]/repo.git"),
            Some(("fe80::1", 80))
        );
        assert_eq!(parse_host_port("ftp://example.com"), None);
        assert_eq!(parse_host_port("git@github.com:foo/bar.git"), None);
    }

    #[test]
    fn blocks_forbidden_ips() {
        use std::net::{Ipv4Addr, Ipv6Addr};

        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(
            169, 254, 169, 254
        ))));
        assert!(is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))));

        assert!(is_forbidden_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_forbidden_ip(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
        assert!(is_forbidden_ip(IpAddr::V6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        ))));
        assert!(is_forbidden_ip(IpAddr::V6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        ))));
        assert!(is_forbidden_ip(IpAddr::V6(Ipv6Addr::new(
            0xfd12, 0x3456, 0x789a, 0, 0, 0, 0, 1
        ))));
        assert!(is_forbidden_ip(IpAddr::V6(
            Ipv4Addr::new(127, 0, 0, 1).to_ipv6_mapped()
        )));
        assert!(is_forbidden_ip(IpAddr::V6(
            Ipv4Addr::new(192, 168, 1, 1).to_ipv6_mapped()
        )));

        assert!(!is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_forbidden_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
        assert!(!is_forbidden_ip(IpAddr::V6(Ipv6Addr::new(
            0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111
        ))));
    }

    #[tokio::test]
    async fn validates_clone_urls_against_ssrf() {
        assert!(
            validate_clone_url("https://127.0.0.1/test.git")
                .await
                .is_err()
        );
        assert!(
            validate_clone_url("http://localhost/test.git")
                .await
                .is_err()
        );
        assert!(
            validate_clone_url("http://internal.service.local/test.git")
                .await
                .is_err()
        );
        assert!(validate_clone_url("http://[::1]/test.git").await.is_err());
        assert!(validate_clone_url("file:///etc/passwd").await.is_err());
    }
}
