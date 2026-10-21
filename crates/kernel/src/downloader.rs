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

use std::sync::atomic::AtomicBool;

use crate::{LumiError, Result};

const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
const CHANGELOG_LIMIT: usize = 20;
const DIFF_STAT_LIMIT: usize = 30;
const SHORT_SHA: usize = 7;

fn git_error(context: &str, error: impl std::fmt::Display) -> LumiError {
    LumiError::Other(format!("{context}: {error}"))
}

fn open(dir: &Path) -> Result<gix::Repository> {
    gix::open(dir).map_err(|error| git_error("failed to open repository", error))
}

async fn blocking<T, F>(what: &'static str, work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    crate::retry::with_timeout(what, GIT_TIMEOUT, async {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|error| git_error(what, error))?
    })
    .await
}

fn clone_blocking(url: &str, dir: &Path, branch: Option<&str>) -> Result<()> {
    let mut prepare = gix::prepare_clone(url, dir)
        .map_err(|error| git_error("failed to prepare clone", error))?;
    if let Some(branch) = branch {
        prepare = prepare
            .with_ref_name(Some(branch))
            .map_err(|error| git_error("invalid branch", error))?;
    }
    let interrupt = AtomicBool::new(false);
    let (mut checkout, _) = prepare
        .fetch_then_checkout(gix::progress::Discard, &interrupt)
        .map_err(|error| git_error("git clone failed", error))?;
    checkout
        .main_worktree(gix::progress::Discard, &interrupt)
        .map_err(|error| git_error("git checkout failed", error))?;
    Ok(())
}

fn fetch_blocking(repo: &gix::Repository) -> Result<()> {
    let remote = repo
        .find_remote("origin")
        .map_err(|error| git_error("repository has no origin remote", error))?;
    let mut progress = gix::progress::Discard;
    remote
        .connect(gix::remote::Direction::Fetch)
        .map_err(|error| git_error("failed to connect to origin", error))?
        .prepare_fetch(&mut progress, gix::remote::ref_map::Options::default())
        .map_err(|error| git_error("git fetch failed", error))?
        .receive(&mut progress, &gix::interrupt::IS_INTERRUPTED)
        .map_err(|error| git_error("git fetch failed", error))?;
    Ok(())
}

fn head_sha_blocking(repo: &gix::Repository) -> Result<String> {
    repo.head_id()
        .map(|id| id.detach().to_string())
        .map_err(|error| git_error("failed to read HEAD", error))
}

fn branch_name(repo: &gix::Repository) -> Option<String> {
    repo.head_name()
        .ok()
        .flatten()
        .map(|name| name.shorten().to_string())
}

fn short_sha(id: &gix::ObjectId) -> String {
    id.to_string().chars().take(SHORT_SHA).collect()
}

fn clear_worktree(dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(dir).map_err(LumiError::Io)? {
        let entry = entry.map_err(LumiError::Io)?;
        if entry.file_name() == ".git" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path).map_err(LumiError::Io)?;
        } else {
            std::fs::remove_file(&path).map_err(LumiError::Io)?;
        }
    }
    Ok(())
}

fn reset_blocking(repo: &gix::Repository, commit: gix::ObjectId) -> Result<()> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| LumiError::Other("repository has no worktree".to_string()))?
        .to_path_buf();
    let tree = repo
        .find_commit(commit)
        .map_err(|error| git_error("failed to read commit", error))?
        .tree()
        .map_err(|error| git_error("failed to read tree", error))?
        .id;

    clear_worktree(&workdir)?;
    let mut index = repo
        .index_from_tree(&tree)
        .map_err(|error| git_error("failed to build index", error))?;
    let mut options = repo
        .checkout_options(gix::worktree::stack::state::attributes::Source::IdMapping)
        .map_err(|error| git_error("failed to configure checkout", error))?;
    options.destination_is_initially_empty = true;
    let interrupt = AtomicBool::new(false);
    gix_worktree_state::checkout(
        &mut index,
        &workdir,
        repo.objects
            .clone()
            .into_arc()
            .map_err(|error| git_error("failed to share object database", error))?,
        &gix::progress::Discard,
        &gix::progress::Discard,
        &interrupt,
        options,
    )
    .map_err(|error| git_error("git checkout failed", error))?;
    index
        .write(gix::index::write::Options::default())
        .map_err(|error| git_error("failed to write index", error))
}

fn fast_forward_blocking(dir: &Path) -> Result<String> {
    let repo = open(dir)?;
    if repo
        .is_dirty()
        .map_err(|error| git_error("failed to inspect worktree", error))?
    {
        return Err(LumiError::Invalid(
            "repository has uncommitted changes".to_string(),
        ));
    }
    let branch = branch_name(&repo).unwrap_or_else(|| "main".to_string());
    fetch_blocking(&repo)?;

    let target = repo
        .rev_parse_single(format!("refs/remotes/origin/{branch}").as_str())
        .map_err(|error| git_error("failed to read origin branch", error))?
        .detach();
    let head = repo
        .head_id()
        .map_err(|error| git_error("failed to read HEAD", error))?
        .detach();
    if head == target {
        return Ok(target.to_string());
    }
    let base = repo
        .merge_base(head, target)
        .map_err(|error| git_error("failed to find merge base", error))?
        .map(gix::Id::detach);
    if base != Some(head) {
        return Err(LumiError::Invalid(
            "origin has diverged from the local repository".to_string(),
        ));
    }
    reset_blocking(&repo, target)?;
    repo.reference(
        format!("refs/heads/{branch}").as_str(),
        target,
        gix::refs::transaction::PreviousValue::Any,
        "lumi pull",
    )
    .map_err(|error| git_error("failed to move branch", error))?;
    Ok(target.to_string())
}

fn changelog_blocking(repo: &gix::Repository, from: &str, to: &str) -> Result<String> {
    let head = repo
        .rev_parse_single(from)
        .map_err(|error| git_error("failed to resolve range start", error))?
        .detach();
    let target = repo
        .rev_parse_single(to)
        .map_err(|error| git_error("failed to resolve range end", error))?
        .detach();
    let mut lines = Vec::new();
    for step in repo
        .rev_walk([target])
        .all()
        .map_err(|error| git_error("failed to walk history", error))?
        .take(CHANGELOG_LIMIT)
    {
        let step = step.map_err(|error| git_error("failed to walk history", error))?;
        if step.id == head {
            break;
        }
        let commit = repo
            .find_object(step.id)
            .map_err(|error| git_error("failed to read commit", error))?
            .into_commit();
        let summary = commit
            .message()
            .map_err(|error| git_error("failed to read commit message", error))?
            .summary()
            .to_string();
        lines.push(format!("{} {summary}", short_sha(&step.id)));
    }
    Ok(lines.join("\n"))
}

fn diff_stat_blocking(repo: &gix::Repository, from: &str, to: &str) -> Result<String> {
    let old = repo
        .rev_parse_single(from)
        .map_err(|error| git_error("failed to resolve range start", error))?
        .detach();
    let new = repo
        .rev_parse_single(to)
        .map_err(|error| git_error("failed to resolve range end", error))?
        .detach();
    let old_tree = repo
        .find_commit(old)
        .map_err(|error| git_error("failed to read commit", error))?
        .tree()
        .map_err(|error| git_error("failed to read tree", error))?;
    let new_tree = repo
        .find_commit(new)
        .map_err(|error| git_error("failed to read commit", error))?
        .tree()
        .map_err(|error| git_error("failed to read tree", error))?;

    let changes = repo
        .diff_tree_to_tree(&old_tree, &new_tree, None)
        .map_err(|error| git_error("failed to diff trees", error))?;
    let mut lines: Vec<String> = changes
        .iter()
        .take(DIFF_STAT_LIMIT)
        .map(|change| match change {
            gix::object::tree::diff::ChangeDetached::Addition { location, .. } => {
                format!("A {location}")
            }
            gix::object::tree::diff::ChangeDetached::Deletion { location, .. } => {
                format!("D {location}")
            }
            gix::object::tree::diff::ChangeDetached::Rewrite { location, .. }
            | gix::object::tree::diff::ChangeDetached::Modification { location, .. } => {
                format!("M {location}")
            }
        })
        .collect();
    if changes.len() > DIFF_STAT_LIMIT {
        lines.push(format!("... {} more", changes.len() - DIFF_STAT_LIMIT));
    }
    Ok(lines.join("\n"))
}

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

        let url_owned = url.to_string();
        let branch_owned = branch.map(str::to_string);
        let dir = repo_dir.clone();
        blocking("git clone", move || {
            clone_blocking(&url_owned, &dir, branch_owned.as_deref())
        })
        .await?;

        let (resolved_branch, commit_hash) = {
            let dir = repo_dir.clone();
            blocking("git inspect", move || {
                let repo = open(&dir)?;
                Ok((
                    branch_name(&repo).unwrap_or_else(|| "main".to_string()),
                    head_sha_blocking(&repo).ok(),
                ))
            })
            .await?
        };

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
        let dir = repo_dir.clone();
        let new_commit = blocking("git pull", move || fast_forward_blocking(&dir)).await?;

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

        let branch = repo.branch.clone();
        let dir = repo_dir.clone();
        let checked = blocking("git update check", move || {
            let repo = open(&dir)?;
            // A failed fetch just compares against the refs we already have.
            let _ = fetch_blocking(&repo);
            let local = head_sha_blocking(&repo)?;
            let target_ref = format!("refs/remotes/origin/{branch}");
            let remote = repo
                .rev_parse_single(target_ref.as_str())
                .map_err(|error| git_error("failed to read origin branch", error))?
                .detach()
                .to_string();
            if local == remote {
                return Ok(None);
            }
            let changelog = changelog_blocking(&repo, "HEAD", target_ref.as_str())?;
            Ok::<_, LumiError>(Some(changelog))
        })
        .await?;

        Ok(match checked {
            Some(changelog) => RepoUpdateCheck {
                ok: true,
                reason: String::new(),
                has_update: true,
                changelog: (!changelog.is_empty()).then_some(changelog),
            },
            None => RepoUpdateCheck {
                ok: true,
                reason: "up to date".to_string(),
                has_update: false,
                changelog: None,
            },
        })
    }

    pub async fn update_repo(&self, name: &str) -> Result<RepoUpdateResult> {
        let repo = self.find_repo(name).await?;
        let old_sha = repo.commit.clone();
        let new_sha = self.pull_repo(name).await?;
        let changed = old_sha.as_deref() != Some(new_sha.as_str());

        let diff_stat = match (changed, old_sha.clone()) {
            (true, Some(old)) => {
                let dir = self.repo_dir(name)?;
                let new = new_sha.clone();
                blocking("git diff", move || {
                    let repo = open(&dir)?;
                    diff_stat_blocking(&repo, &old, &new)
                })
                .await
                .ok()
                .filter(|stat| !stat.is_empty())
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
