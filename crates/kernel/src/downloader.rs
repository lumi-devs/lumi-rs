use std::path::{Path, PathBuf};

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use lumi_db::entities::{downloader_module, downloader_repo};

use crate::error::{LumiError, Result};

#[derive(Clone, Debug)]
pub struct Downloader {
    db: DatabaseConnection,
    addons_dir: PathBuf,
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
