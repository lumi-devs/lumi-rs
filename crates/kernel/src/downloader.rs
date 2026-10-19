use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use lumi_db::entities::{downloader_module, downloader_repo, global, global_module_state};

use crate::error::{LumiError, Result};

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

impl Downloader {
    pub fn new(db: DatabaseConnection, addons_dir: PathBuf) -> Self {
        Self { db, addons_dir }
    }

    pub fn addons_dir(&self) -> &Path {
        &self.addons_dir
    }

    pub async fn add_repo(
        &self,
        url: &str,
        name: Option<&str>,
        branch: Option<&str>,
    ) -> Result<downloader_repo::Model> {
        let repo_name = match name {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => {
                let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
                let seg = trimmed.split('/').last().unwrap_or_default();
                if seg.is_empty() {
                    return Err(LumiError::Other(
                        "could not derive repository name from URL".into(),
                    ));
                }
                seg.to_string()
            }
        };

        let existing = downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(&repo_name))
            .one(&self.db)
            .await?;

        if existing.is_some() {
            return Err(LumiError::Other(format!(
                "repository '{repo_name}' already exists"
            )));
        }

        let repos_dir = self.addons_dir.join("repos");
        tokio::fs::create_dir_all(&repos_dir).await?;
        let repo_dir = repos_dir.join(&repo_name);

        if repo_dir.exists() {
            let _ = tokio::fs::remove_dir_all(&repo_dir).await;
        }

        let repo_dir_clone = repo_dir.clone();
        let url_string = url.to_string();
        let branch_clone = branch.map(ToString::to_string);

        let (actual_branch, commit_hash) =
            tokio::task::spawn_blocking(move || -> Result<(String, Option<String>)> {
                let mut clone_args = vec!["clone".to_string()];
                if let Some(ref b) = branch_clone {
                    clone_args.push("-b".to_string());
                    clone_args.push(b.clone());
                }
                clone_args.push("--".to_string());
                clone_args.push(url_string);
                clone_args.push(repo_dir_clone.to_string_lossy().to_string());

                let mut cmd = std::process::Command::new("git");
                cmd.args(&clone_args);
                cmd.env("GIT_TERMINAL_PROMPT", "0");
                let output = cmd.output().map_err(LumiError::Io)?;
                if !output.status.success() {
                    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    return Err(LumiError::Other(format!("git clone failed: {err}")));
                }

                let mut branch_cmd = std::process::Command::new("git");
                branch_cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);
                branch_cmd.current_dir(&repo_dir_clone);
                let branch_out = branch_cmd.output().map_err(LumiError::Io)?;
                let resolved_branch = if branch_out.status.success() {
                    let b = String::from_utf8_lossy(&branch_out.stdout).trim().to_string();
                    if b.is_empty() {
                        branch_clone.unwrap_or_else(|| "main".to_string())
                    } else {
                        b
                    }
                } else {
                    branch_clone.unwrap_or_else(|| "main".to_string())
                };

                let mut rev_cmd = std::process::Command::new("git");
                rev_cmd.args(["rev-parse", "HEAD"]);
                rev_cmd.current_dir(&repo_dir_clone);
                let rev_out = rev_cmd.output().map_err(LumiError::Io)?;
                let resolved_commit = if rev_out.status.success() {
                    let c = String::from_utf8_lossy(&rev_out.stdout).trim().to_string();
                    if c.is_empty() {
                        None
                    } else {
                        Some(c)
                    }
                } else {
                    None
                };

                Ok((resolved_branch, resolved_commit))
            })
            .await
            .map_err(|e| LumiError::Other(e.to_string()))??;

        let now = Utc::now().fixed_offset();
        let new_repo = downloader_repo::ActiveModel {
            name: Set(repo_name),
            url: Set(url.to_string()),
            branch: Set(actual_branch),
            commit: Set(commit_hash),
            signed_by: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };

        let inserted = new_repo.insert(&self.db).await?;
        Ok(inserted)
    }

    pub async fn pull_repo(&self, name: &str) -> Result<String> {
        let repo = downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await?
            .ok_or_else(|| LumiError::Other(format!("repository '{name}' not found")))?;

        let repo_dir = self.addons_dir.join("repos").join(name);
        if !repo_dir.exists() {
            return Err(LumiError::Other(format!(
                "repository directory for '{name}' does not exist"
            )));
        }

        let repo_dir_clone = repo_dir.clone();
        let new_commit = tokio::task::spawn_blocking(move || -> Result<String> {
            let mut pull_cmd = std::process::Command::new("git");
            pull_cmd.args(["pull"]);
            pull_cmd.current_dir(&repo_dir_clone);
            pull_cmd.env("GIT_TERMINAL_PROMPT", "0");
            let pull_out = pull_cmd.output().map_err(LumiError::Io)?;
            if !pull_out.status.success() {
                let err = String::from_utf8_lossy(&pull_out.stderr).trim().to_string();
                return Err(LumiError::Other(format!("git pull failed: {err}")));
            }

            let mut rev_cmd = std::process::Command::new("git");
            rev_cmd.args(["rev-parse", "HEAD"]);
            rev_cmd.current_dir(&repo_dir_clone);
            let rev_out = rev_cmd.output().map_err(LumiError::Io)?;
            if rev_out.status.success() {
                Ok(String::from_utf8_lossy(&rev_out.stdout).trim().to_string())
            } else {
                Err(LumiError::Other(
                    "failed to resolve commit hash after pull".into(),
                ))
            }
        })
        .await
        .map_err(|e| LumiError::Other(e.to_string()))??;

        let mut active: downloader_repo::ActiveModel = repo.into();
        active.commit = Set(Some(new_commit.clone()));
        active.updated_at = Set(Utc::now().fixed_offset());
        active.update(&self.db).await?;

        Ok(new_commit)
    }

    pub async fn list_repos(&self) -> Result<Vec<downloader_repo::Model>> {
        downloader_repo::Entity::find()
            .order_by_asc(downloader_repo::Column::Name)
            .all(&self.db)
            .await
            .map_err(LumiError::Db)
    }

    pub async fn remove_repo(&self, name: &str) -> Result<bool> {
        let repo = downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await?;

        let Some(repo) = repo else {
            return Ok(false);
        };

        let modules = downloader_module::Entity::find()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .all(&self.db)
            .await?;

        for m in &modules {
            let target_dir = self.addons_dir.join(&m.module_name);
            if target_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&target_dir).await;
            }
            let target_file = self.addons_dir.join(format!("{}.rhai", m.module_name));
            if target_file.exists() {
                let _ = tokio::fs::remove_file(&target_file).await;
            }
        }

        downloader_module::Entity::delete_many()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .exec(&self.db)
            .await?;

        downloader_repo::Entity::delete_by_id(repo.id)
            .exec(&self.db)
            .await?;

        let repo_dir = self.addons_dir.join("repos").join(name);
        if repo_dir.exists() {
            let _ = tokio::fs::remove_dir_all(&repo_dir).await;
        }

        Ok(true)
    }

    pub async fn install_module(
        &self,
        repo_name: &str,
        module_name: &str,
    ) -> Result<downloader_module::Model> {
        let repo = downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(repo_name))
            .one(&self.db)
            .await?
            .ok_or_else(|| LumiError::Other(format!("repository '{repo_name}' not found")))?;

        let repo_dir = self.addons_dir.join("repos").join(repo_name);
        if !repo_dir.exists() {
            return Err(LumiError::Other(format!(
                "repository directory for '{repo_name}' does not exist"
            )));
        }

        let source_dir = repo_dir.join(module_name);
        let source_file = repo_dir.join(format!("{module_name}.rhai"));

        let is_dir = source_dir.is_dir();
        let is_file = source_file.is_file();

        if !is_dir && !is_file {
            return Err(LumiError::Other(format!(
                "module '{module_name}' not found in repository '{repo_name}'"
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
            } else if is_file {
                let target_file = addons_dir.join(format!("{mod_name}.rhai"));
                std::fs::copy(&source_file, &target_file).map_err(LumiError::Io)?;
            }
            Ok(())
        })
        .await
        .map_err(|e| LumiError::Other(e.to_string()))??;

        let manifest_path = repo_dir.join(module_name).join("manifest.json");
        let version = if manifest_path.exists() {
            tokio::fs::read_to_string(&manifest_path)
                .await
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    v.get("version")
                        .and_then(|ver| ver.as_str())
                        .map(ToString::to_string)
                })
        } else {
            None
        };

        let existing = downloader_module::Entity::find()
            .filter(downloader_module::Column::RepoId.eq(repo.id))
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?;

        let now = Utc::now().fixed_offset();
        let model = if let Some(m) = existing {
            let mut active: downloader_module::ActiveModel = m.into();
            active.version = Set(version);
            active.commit = Set(repo.commit.clone());
            active.installed_at = Set(now);
            active.update(&self.db).await?
        } else {
            let active = downloader_module::ActiveModel {
                repo_id: Set(repo.id),
                module_name: Set(module_name.to_string()),
                version: Set(version),
                commit: Set(repo.commit.clone()),
                signed_by: Set(None),
                pinned: Set(false),
                installed_at: Set(now),
            };
            active.insert(&self.db).await?
        };

        Ok(model)
    }

    pub async fn list_installed_modules(&self) -> Result<Vec<downloader_module::Model>> {
        downloader_module::Entity::find()
            .order_by_asc(downloader_module::Column::ModuleName)
            .all(&self.db)
            .await
            .map_err(LumiError::Db)
    }

    pub async fn get_repo_by_id(&self, id: i32) -> Result<Option<downloader_repo::Model>> {
        downloader_repo::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(LumiError::Db)
    }

    pub async fn get_repo_by_name(&self, name: &str) -> Result<Option<downloader_repo::Model>> {
        downloader_repo::Entity::find()
            .filter(downloader_repo::Column::Name.eq(name))
            .one(&self.db)
            .await
            .map_err(LumiError::Db)
    }

    /// Whether a module has been disabled globally. Defaults to enabled.
    pub async fn is_module_enabled(&self, module_name: &str) -> anyhow::Result<bool> {
        let row = global_module_state::Entity::find()
            .filter(global_module_state::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?;
        Ok(row.is_none_or(|m| m.enabled))
    }

    /// Lazily compare a repository's remote branch against its local checkout.
    pub async fn check_repo_update(&self, name: &str) -> anyhow::Result<RepoUpdateCheck> {
        let Some(repo) = self.get_repo_by_name(name).await? else {
            return Ok(RepoUpdateCheck {
                ok: false,
                reason: format!("Repository **{name}** was not found."),
                has_update: false,
                changelog: None,
            });
        };

        let repo_dir = self.addons_dir.join("repos").join(name);
        if !repo_dir.exists() {
            return Ok(RepoUpdateCheck {
                ok: true,
                reason: String::new(),
                has_update: true,
                changelog: None,
            });
        }

        let branch = repo.branch.clone();
        let dir = repo_dir.clone();
        let (local, remote, changelog) =
            tokio::task::spawn_blocking(move || -> Result<(String, String, String)> {
                // why: a failed fetch is tolerated; we then compare against stale refs.
                let _ = git(&dir, &["fetch", "origin"]);
                let local = git(&dir, &["rev-parse", "HEAD"])?;
                let upstream =
                    git(&dir, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap_or_default();
                let target = if upstream.is_empty() || upstream.contains("@{u}") {
                    format!("origin/{branch}")
                } else {
                    upstream
                };
                let remote = git(&dir, &["rev-parse", target.as_str()])?;
                let range = format!("HEAD..{target}");
                let changelog =
                    git(&dir, &["log", "--oneline", range.as_str()]).unwrap_or_default();
                Ok((local, remote, changelog))
            })
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))??;

        if local == remote {
            return Ok(RepoUpdateCheck {
                ok: true,
                reason: "up to date".to_string(),
                has_update: false,
                changelog: None,
            });
        }

        Ok(RepoUpdateCheck {
            ok: true,
            reason: String::new(),
            has_update: true,
            changelog: if changelog.is_empty() {
                None
            } else {
                Some(changelog)
            },
        })
    }

    /// Pull a repository and report what changed.
    pub async fn update_repo(&self, name: &str) -> anyhow::Result<RepoUpdateResult> {
        let repo = self
            .get_repo_by_name(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("repository '{name}' not found"))?;
        let old_sha = repo.commit.clone();
        let new_sha = self.pull_repo(name).await?;
        let changed = old_sha.as_deref() != Some(new_sha.as_str());

        let diff_stat = match (changed, old_sha.clone()) {
            (true, Some(old)) => {
                let repo_dir = self.addons_dir.join("repos").join(name);
                let range = format!("{old}..{new_sha}");
                tokio::task::spawn_blocking(move || {
                    git(&repo_dir, &["diff", "--stat", range.as_str()]).ok()
                })
                .await
                .ok()
                .flatten()
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

    /// Count repositories whose remote branch is ahead of the local checkout.
    /// Names of tracked repositories that have a pending update.
    pub async fn check_for_updates_detailed(&self) -> anyhow::Result<Vec<String>> {
        let modules = self.list_installed_modules().await?;
        let repos = self.list_repos().await?;
        let names: HashMap<i32, String> = repos.into_iter().map(|r| (r.id, r.name)).collect();

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
    pub async fn uninstall_module(&self, module_name: &str) -> anyhow::Result<bool> {
        let Some(model) = downloader_module::Entity::find()
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?
        else {
            return Ok(false);
        };

        let target_dir = self.addons_dir.join(module_name);
        if target_dir.exists() {
            let _ = tokio::fs::remove_dir_all(&target_dir).await;
        }
        let target_file = self.addons_dir.join(format!("{module_name}.rhai"));
        if target_file.exists() {
            let _ = tokio::fs::remove_file(&target_file).await;
        }

        downloader_module::Entity::delete_many()
            .filter(downloader_module::Column::RepoId.eq(model.repo_id))
            .filter(downloader_module::Column::ModuleName.eq(module_name))
            .exec(&self.db)
            .await?;
        Ok(true)
    }

    /// Enable or disable a module globally.
    pub async fn toggle_module(&self, module_name: &str, enabled: bool) -> anyhow::Result<()> {
        let now = Utc::now().fixed_offset();
        let reason = Some("toggled via addons panel".to_string());
        match global_module_state::Entity::find()
            .filter(global_module_state::Column::ModuleName.eq(module_name))
            .one(&self.db)
            .await?
        {
            Some(existing) => {
                let mut active: global_module_state::ActiveModel = existing.into();
                active.enabled = Set(enabled);
                active.reason = Set(reason);
                active.updated_at = Set(now);
                active.update(&self.db).await?;
            }
            None => {
                global_module_state::ActiveModel {
                    module_name: Set(module_name.to_string()),
                    enabled: Set(enabled),
                    reason: Set(reason),
                    updated_at: Set(now),
                }
                .insert(&self.db)
                .await?;
            }
        }
        Ok(())
    }

    /// Installed modules joined with their source repository name.
    pub async fn get_installed_modules_detailed(&self) -> anyhow::Result<Vec<InstalledModule>> {
        let modules = self.list_installed_modules().await?;
        let repos = self.list_repos().await?;
        let names: HashMap<i32, String> = repos.into_iter().map(|r| (r.id, r.name)).collect();

        Ok(modules
            .into_iter()
            .filter_map(|m| {
                Some(InstalledModule {
                    module_name: m.module_name,
                    version: m.version,
                    repo_name: names.get(&m.repo_id)?.clone(),
                    installed_at: m.installed_at,
                })
            })
            .collect())
    }

    /// Scan a repository checkout for module manifests.
    pub async fn get_modules_in_repo(&self, repo_name: &str) -> anyhow::Result<Vec<RepoModuleInfo>> {
        let repo_dir = self.addons_dir.join("repos").join(repo_name);
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
            let Ok(raw) = tokio::fs::read_to_string(entry.path().join("manifest.json")).await else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            out.push(RepoModuleInfo {
                name: v
                    .get("name")
                    .and_then(|x| x.as_str())
                    .map_or_else(|| dir_name.clone(), ToString::to_string),
                version: v
                    .get("version")
                    .and_then(|x| x.as_str())
                    .unwrap_or("1.0.0")
                    .to_string(),
                short: v.get("short").and_then(|x| x.as_str()).map(ToString::to_string),
                description: v
                    .get("description")
                    .and_then(|x| x.as_str())
                    .map(ToString::to_string),
                author: v.get("author").and_then(render_author),
                min_bot_version: v
                    .get("min_bot_version")
                    .and_then(|x| x.as_str())
                    .map(ToString::to_string),
                hidden: v
                    .get("hidden")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                end_user_data_statement: v
                    .get("end_user_data_statement")
                    .and_then(|x| x.as_str())
                    .map(ToString::to_string),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    /// Read the auto-update settings from `global_config`.
    pub async fn get_auto_update_config(&self) -> anyhow::Result<AutoUpdateConfig> {
        let row = global::Entity::find()
            .order_by_asc(global::Column::Id)
            .one(&self.db)
            .await?;
        Ok(row.map_or(
            AutoUpdateConfig {
                enabled: false,
                interval_minutes: 1440,
            },
            |r| AutoUpdateConfig {
                enabled: r.auto_update_enabled,
                interval_minutes: u64::try_from(r.auto_update_interval_minutes).unwrap_or(1440),
            },
        ))
    }

    /// Write the auto-update settings to the existing `global_config` row.
    pub async fn set_auto_update_config(&self, cfg: AutoUpdateConfig) -> anyhow::Result<()> {
        let row = global::Entity::find()
            .order_by_asc(global::Column::Id)
            .one(&self.db)
            .await?;
        if let Some(existing) = row {
            let mut active: global::ActiveModel = existing.into();
            active.auto_update_enabled = Set(cfg.enabled);
            active.auto_update_interval_minutes =
                Set(i32::try_from(cfg.interval_minutes).unwrap_or(i32::MAX));
            active.updated_at = Set(Utc::now().fixed_offset());
            active.update(&self.db).await?;
        }
        Ok(())
    }
}

/// Run `git` in `dir`, returning trimmed stdout or an error carrying stderr.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
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
