//! Addon repository management: git-backed installs of external modules.
//!
//! Every git invocation goes through the async [`git`] helper; only the
//! recursive directory copy runs on the blocking pool.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use lumi_db::entities::{downloader_module, downloader_repo, global, global_module_state};

use crate::{LumiError, Result};

#[derive(Clone, Debug)]
pub struct Downloader {
    db: DatabaseConnection,
    addons_dir: PathBuf,
}

/// Outcome of a remote-vs-local update check for one repository.
#[derive(Clone, Debug)]
pub struct RepoUpdateCheck {
    pub ok: bool,
    pub reason: String,
    pub has_update: bool,
    pub changelog: Option<String>,
}

/// Outcome of pulling a repository.
#[derive(Clone, Debug)]
pub struct RepoUpdateResult {
    pub changed: bool,
    pub old_sha: Option<String>,
    pub new_sha: String,
    pub diff_stat: Option<String>,
}

/// An installed module joined with its source repository.
#[derive(Clone, Debug)]
pub struct InstalledModule {
    pub module_name: String,
    pub version: Option<String>,
    pub repo_name: String,
    pub installed_at: chrono::DateTime<chrono::FixedOffset>,
}

/// One module manifest discovered inside a repository checkout.
#[derive(Clone, Debug)]
pub struct RepoModuleInfo {
    pub name: String,
    pub version: String,
    pub short: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub min_bot_version: Option<String>,
    pub hidden: bool,
    pub end_user_data_statement: Option<String>,
}

/// Auto-update settings, persisted in `global_config`.
#[derive(Clone, Debug)]
pub struct AutoUpdateConfig {
    pub enabled: bool,
    pub interval_minutes: u64,
}

/// Reject names that could escape the addons directory when joined as a path.
fn safe_component(name: &str) -> Result<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(['/', '\\', '\0'])
    {
        return Err(LumiError::Invalid(format!("'{name}' is not a valid name")));
    }
    Ok(trimmed)
}

/// Run `git` in `dir`, returning trimmed stdout or an error carrying stderr.
async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .await
        .map_err(LumiError::Io)?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(LumiError::Other(format!(
            "git {} failed: {err}",
            args.join(" ")
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Best-effort removal of a file or directory; missing paths are ignored.
async fn remove_path(path: &Path) {
    match tokio::fs::remove_dir_all(path).await {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            if let Err(err) = tokio::fs::remove_file(path).await {
                if err.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), error = %err, "failed to remove path");
                }
            }
        }
    }
}

impl Downloader {
    pub fn new(db: DatabaseConnection, addons_dir: PathBuf) -> Self {
        Self { db, addons_dir }
    }

    pub fn addons_dir(&self) -> &Path {
        &self.addons_dir
    }

    fn repo_dir(&self, name: &str) -> Result<PathBuf> {
        Ok(self.addons_dir.join("repos").join(safe_component(name)?))
    }

    async fn find_repo(&self, name: &str) -> Result<downloader_repo::Model> {
        downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await?
            .ok_or_else(|| LumiError::NotFound(format!("repository '{name}'")))
    }

    async fn repo_names(&self) -> Result<HashMap<i32, String>> {
        Ok(self
            .list_repos()
            .await?
            .into_iter()
            .map(|repo| (repo.id, repo.name))
            .collect())
    }

    pub async fn add_repo(
        &self,
        url: &str,
        name: Option<&str>,
        branch: Option<&str>,
    ) -> Result<downloader_repo::Model> {
        let derived = url
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit('/')
            .next()
            .unwrap_or_default();
        let candidate = match name {
            Some(n) if !n.trim().is_empty() => n,
            _ => derived,
        };
        let repo_name = safe_component(candidate)?.to_string();

        if downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(&repo_name))
            .one(&self.db)
            .await?
            .is_some()
        {
            return Err(LumiError::Invalid(format!(
                "repository '{repo_name}' already exists"
            )));
        }

        let repos_dir = self.addons_dir.join("repos");
        tokio::fs::create_dir_all(&repos_dir).await?;
        let repo_dir = repos_dir.join(&repo_name);
        remove_path(&repo_dir).await;

        let repo_path = repo_dir.to_string_lossy().to_string();
        let mut args = vec!["clone"];
        if let Some(branch) = branch {
            args.push("-b");
            args.push(branch);
        }
        args.push("--");
        args.push(url);
        args.push(&repo_path);
        git(&repos_dir, &args).await?;

        let resolved_branch = git(&repo_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .ok()
            .filter(|resolved| !resolved.is_empty())
            .or_else(|| branch.map(str::to_string))
            .unwrap_or_else(|| "main".to_string());
        let commit_hash = git(&repo_dir, &["rev-parse", "HEAD"])
            .await
            .ok()
            .filter(|commit| !commit.is_empty());

        let now = Utc::now().fixed_offset();
        let inserted = downloader_repo::ActiveModel {
            name: Set(repo_name),
            url: Set(url.to_string()),
            branch: Set(resolved_branch),
            commit: Set(commit_hash),
            signed_by: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await?;
        Ok(inserted)
    }

    pub async fn pull_repo(&self, name: &str) -> Result<String> {
        let repo = self.find_repo(name).await?;
        let repo_dir = self.repo_dir(name)?;
        if !repo_dir.exists() {
            return Err(LumiError::NotFound(format!(
                "repository directory for '{name}'"
            )));
        }

        git(&repo_dir, &["pull"]).await?;
        let new_commit = git(&repo_dir, &["rev-parse", "HEAD"]).await?;

        let mut active: downloader_repo::ActiveModel = repo.into();
        active.commit = Set(Some(new_commit.clone()));
        active.updated_at = Set(Utc::now().fixed_offset());
        active.update(&self.db).await?;

        Ok(new_commit)
    }

    pub async fn list_repos(&self) -> Result<Vec<downloader_repo::Model>> {
        Ok(downloader_repo::Entity::find()
            .order_by_asc(downloader_repo::Column::Name)
            .all(&self.db)
            .await?)
    }

    pub async fn remove_repo(&self, name: &str) -> Result<bool> {
        let Some(repo) = downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await?
        else {
            return Ok(false);
        };

        for module in downloader_module::Entity::find()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .all(&self.db)
            .await?
        {
            remove_path(&self.addons_dir.join(&module.module_name)).await;
            remove_path(&self.addons_dir.join(format!("{}.rhai", module.module_name))).await;
        }

        downloader_module::Entity::delete_many()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .exec(&self.db)
            .await?;
        downloader_repo::Entity::delete_by_id(repo.id)
            .exec(&self.db)
            .await?;
        remove_path(&self.repo_dir(name)?).await;

        Ok(true)
    }

    pub async fn install_module(
        &self,
        repo_name: &str,
        module_name: &str,
    ) -> Result<downloader_module::Model> {
        let repo_name = safe_component(repo_name)?;
        let module_name = safe_component(module_name)?;
        let repo = self.find_repo(repo_name).await?;

        let repo_dir = self.repo_dir(repo_name)?;
        if !repo_dir.exists() {
            return Err(LumiError::NotFound(format!(
                "repository directory for '{repo_name}'"
            )));
        }

        let source_dir = repo_dir.join(module_name);
        let source_file = repo_dir.join(format!("{module_name}.rhai"));
        let is_dir = source_dir.is_dir();
        if !is_dir && !source_file.is_file() {
            return Err(LumiError::NotFound(format!(
                "module '{module_name}' in repository '{repo_name}'"
            )));
        }

        let addons_dir = self.addons_dir.clone();
        let mod_name = module_name.to_string();
        tokio::task::spawn_blocking(move || -> Result<()> {
            std::fs::create_dir_all(&addons_dir).map_err(LumiError::Io)?;
            if is_dir {
                let target_dir = addons_dir.join(&mod_name);
                if target_dir.exists() {
                    let _ = std::fs::remove_dir_all(&target_dir);
                }
                copy_dir_recursive(&source_dir, &target_dir).map_err(LumiError::Io)?;
            } else {
                std::fs::copy(&source_file, addons_dir.join(format!("{mod_name}.rhai")))
                    .map_err(LumiError::Io)?;
            }
            Ok(())
        })
        .await??;

        let version =
            read_manifest_version(&repo_dir.join(module_name).join("manifest.json")).await;

        let existing = downloader_module::Entity::find()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?;

        let now = Utc::now().fixed_offset();
        let model = if let Some(module) = existing {
            let mut active: downloader_module::ActiveModel = module.into();
            active.version = Set(version);
            active.commit = Set(repo.commit.clone());
            active.installed_at = Set(now);
            active.update(&self.db).await?
        } else {
            downloader_module::ActiveModel {
                repo_id: Set(repo.id),
                module_name: Set(module_name.to_string()),
                version: Set(version),
                commit: Set(repo.commit.clone()),
                signed_by: Set(None),
                pinned: Set(false),
                installed_at: Set(now),
            }
            .insert(&self.db)
            .await?
        };

        Ok(model)
    }

    pub async fn list_installed_modules(&self) -> Result<Vec<downloader_module::Model>> {
        Ok(downloader_module::Entity::find()
            .order_by_asc(downloader_module::Column::ModuleName)
            .all(&self.db)
            .await?)
    }

    pub async fn get_repo_by_id(&self, id: i32) -> Result<Option<downloader_repo::Model>> {
        Ok(downloader_repo::Entity::find_by_id(id)
            .one(&self.db)
            .await?)
    }

    pub async fn get_repo_by_name(&self, name: &str) -> Result<Option<downloader_repo::Model>> {
        Ok(downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await?)
    }

    /// Whether a module has been disabled globally. Defaults to enabled.
    pub async fn is_module_enabled(&self, module_name: &str) -> Result<bool> {
        let row = global_module_state::Entity::find()
            .filter(global_module_state::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?;
        Ok(row.is_none_or(|module| module.enabled))
    }

    /// Lazily compare a repository's remote branch against its local checkout.
    pub async fn check_repo_update(&self, name: &str) -> Result<RepoUpdateCheck> {
        let Some(repo) = self.get_repo_by_name(name).await? else {
            return Ok(RepoUpdateCheck {
                ok: false,
                reason: format!("Repository **{name}** was not found."),
                has_update: false,
                changelog: None,
            });
        };

        let repo_dir = self.repo_dir(name)?;
        if !repo_dir.exists() {
            return Ok(RepoUpdateCheck {
                ok: true,
                reason: String::new(),
                has_update: true,
                changelog: None,
            });
        }

        // why: a failed fetch is tolerated; we then compare against stale refs.
        let _ = git(&repo_dir, &["fetch", "origin"]).await;
        let local = git(&repo_dir, &["rev-parse", "HEAD"]).await?;
        let upstream = git(&repo_dir, &["rev-parse", "--abbrev-ref", "@{u}"])
            .await
            .unwrap_or_default();
        let target = if upstream.is_empty() || upstream.contains("@{u}") {
            format!("origin/{}", repo.branch)
        } else {
            upstream
        };
        let remote = git(&repo_dir, &["rev-parse", target.as_str()]).await?;
        if local == remote {
            return Ok(RepoUpdateCheck {
                ok: true,
                reason: "up to date".to_string(),
                has_update: false,
                changelog: None,
            });
        }

        let changelog = git(&repo_dir, &["log", "--oneline", &format!("HEAD..{target}")])
            .await
            .unwrap_or_default();
        Ok(RepoUpdateCheck {
            ok: true,
            reason: String::new(),
            has_update: true,
            changelog: (!changelog.is_empty()).then_some(changelog),
        })
    }

    /// Pull a repository and report what changed.
    pub async fn update_repo(&self, name: &str) -> Result<RepoUpdateResult> {
        let repo = self.find_repo(name).await?;
        let old_sha = repo.commit.clone();
        let new_sha = self.pull_repo(name).await?;
        let changed = old_sha.as_deref() != Some(new_sha.as_str());

        let diff_stat = match (changed, old_sha.clone()) {
            (true, Some(old)) => {
                let range = format!("{old}..{new_sha}");
                git(&self.repo_dir(name)?, &["diff", "--stat", &range])
                    .await
                    .ok()
            }
            _ => None,
        };

        Ok(RepoUpdateResult {
            changed,
            old_sha,
            new_sha,
            diff_stat,
        })
    }

    /// Names of tracked repositories whose remote branch is ahead of the local checkout.
    pub async fn check_for_updates_detailed(&self) -> Result<Vec<String>> {
        let modules = self.list_installed_modules().await?;
        let names = self.repo_names().await?;

        let mut seen: HashSet<i32> = HashSet::new();
        let mut pending = Vec::new();
        for module in modules {
            let Some(name) = names.get(&module.repo_id) else {
                continue;
            };
            if !seen.insert(module.repo_id) {
                continue;
            }
            if let Ok(check) = self.check_repo_update(name).await {
                if check.ok && check.has_update {
                    pending.push(name.clone());
                }
            }
        }
        Ok(pending)
    }

    /// Remove a module's files and forget it. Returns whether it was tracked.
    ///
    /// shortcut: `anyhow::Result` to keep the existing `crates/lumi` match arms
    /// type-checking; switch to kernel `Result` once lumi is updated.
    pub async fn uninstall_module(&self, module_name: &str) -> anyhow::Result<bool> {
        let module_name = safe_component(module_name)?;
        if downloader_module::Entity::find()
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?
            .is_none()
        {
            return Ok(false);
        }

        remove_path(&self.addons_dir.join(module_name)).await;
        remove_path(&self.addons_dir.join(format!("{module_name}.rhai"))).await;

        downloader_module::Entity::delete_many()
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .exec(&self.db)
            .await?;
        Ok(true)
    }

    /// Enable or disable a module globally.
    pub async fn toggle_module(&self, module_name: &str, enabled: bool) -> Result<()> {
        global_module_state::Entity::insert(global_module_state::ActiveModel {
            module_name: Set(module_name.to_string()),
            enabled: Set(enabled),
            reason: Set(Some("toggled via addons panel".to_string())),
            updated_at: Set(Utc::now().fixed_offset()),
        })
        .on_conflict(
            sea_orm::sea_query::OnConflict::column(global_module_state::Column::ModuleName)
                .update_columns([
                    global_module_state::Column::Enabled,
                    global_module_state::Column::Reason,
                    global_module_state::Column::UpdatedAt,
                ])
                .to_owned(),
        )
        .exec(&self.db)
        .await?;
        Ok(())
    }

    /// Installed modules joined with their source repository name.
    pub async fn get_installed_modules_detailed(&self) -> Result<Vec<InstalledModule>> {
        let modules = self.list_installed_modules().await?;
        let names = self.repo_names().await?;

        Ok(modules
            .into_iter()
            .filter_map(|module| {
                Some(InstalledModule {
                    module_name: module.module_name,
                    version: module.version,
                    repo_name: names.get(&module.repo_id)?.clone(),
                    installed_at: module.installed_at,
                })
            })
            .collect())
    }

    /// Scan a repository checkout for module manifests.
    pub async fn get_modules_in_repo(&self, repo_name: &str) -> Result<Vec<RepoModuleInfo>> {
        let repo_dir = self.repo_dir(repo_name)?;
        if !repo_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let mut entries = tokio::fs::read_dir(&repo_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if dir_name.starts_with('.') || dir_name.starts_with('_') {
                continue;
            }
            let Ok(raw) = tokio::fs::read_to_string(entry.path().join("manifest.json")).await
            else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            out.push(RepoModuleInfo {
                name: value
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map_or_else(|| dir_name.clone(), ToString::to_string),
                version: value
                    .get("version")
                    .and_then(|x| x.as_str())
                    .unwrap_or("1.0.0")
                    .to_string(),
                short: text_field(&value, "short"),
                description: text_field(&value, "description"),
                author: value.get("author").and_then(render_author),
                min_bot_version: text_field(&value, "min_bot_version"),
                hidden: value
                    .get("hidden")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                end_user_data_statement: text_field(&value, "end_user_data_statement"),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Read the auto-update settings from `global_config`.
    pub async fn get_auto_update_config(&self) -> Result<AutoUpdateConfig> {
        let row = global::Entity::find()
            .order_by_asc(global::Column::Id)
            .one(&self.db)
            .await?;
        Ok(row.map_or(
            AutoUpdateConfig {
                enabled: false,
                interval_minutes: 1440,
            },
            |row| AutoUpdateConfig {
                enabled: row.auto_update_enabled,
                interval_minutes: u64::try_from(row.auto_update_interval_minutes).unwrap_or(1440),
            },
        ))
    }

    /// Write the auto-update settings to the existing `global_config` row.
    pub async fn set_auto_update_config(&self, cfg: AutoUpdateConfig) -> Result<()> {
        let row = global::Entity::find()
            .order_by_asc(global::Column::Id)
            .one(&self.db)
            .await?;
        let Some(existing) = row else {
            tracing::warn!("global config row missing; auto-update settings not persisted");
            return Ok(());
        };
        let mut active: global::ActiveModel = existing.into();
        active.auto_update_enabled = Set(cfg.enabled);
        active.auto_update_interval_minutes =
            Set(i32::try_from(cfg.interval_minutes).unwrap_or(i32::MAX));
        active.updated_at = Set(Utc::now().fixed_offset());
        active.update(&self.db).await?;
        Ok(())
    }
}

fn text_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
}

async fn read_manifest_version(path: &Path) -> Option<String> {
    let raw = tokio::fs::read_to_string(path).await.ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    text_field(&value, "version")
}

/// A manifest `author` is either a string or an array of strings.
fn render_author(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    let names: Vec<String> = value
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(ToString::to_string))
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_component_rejects_traversal() {
        assert_eq!(safe_component("good-name").ok(), Some("good-name"));
        assert!(safe_component("").is_err());
        assert!(safe_component("..").is_err());
        assert!(safe_component("a/b").is_err());
        assert!(safe_component("a\\b").is_err());
        assert!(safe_component(" a ").is_ok());
    }

    #[test]
    fn render_author_accepts_string_and_array() {
        assert_eq!(render_author(&serde_json::json!("A")), Some("A".into()));
        assert_eq!(
            render_author(&serde_json::json!(["A", "B"])),
            Some("A, B".into())
        );
        assert_eq!(render_author(&serde_json::json!([])), None);
    }
}
