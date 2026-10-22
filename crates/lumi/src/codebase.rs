//! Source and dependency counts for `/about`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Files that are never project source.
const SKIPPED_DIRS: [&str; 3] = ["target", ".git", "node_modules"];

/// Workspace root, derived from this crate's manifest so it survives any cwd.
fn workspace_root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .map(Path::to_path_buf)
    })
    .as_deref()
}

/// What `/about` reports about the codebase itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct CodebaseStats {
    /// Total source code lines.
    pub lines: u64,
    /// Total locked workspace dependencies.
    pub dependencies: u64,
}

/// Count Rust source lines and locked dependencies once per process.
pub fn stats() -> CodebaseStats {
    static STATS: OnceLock<CodebaseStats> = OnceLock::new();
    *STATS.get_or_init(|| {
        let Some(root) = workspace_root() else {
            return CodebaseStats::default();
        };
        CodebaseStats {
            lines: count_lines(root),
            dependencies: count_lock_packages(&root.join("Cargo.lock")),
        }
    })
}

fn count_lines(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if !SKIPPED_DIRS.iter().any(|skip| name == *skip) {
                total += count_lines(&path);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            total += std::fs::read_to_string(&path).map_or(0, |source| {
                u64::try_from(source.lines().count()).unwrap_or(u64::MAX)
            });
        }
    }
    total
}

/// Every crate the build resolves, direct or transitive.
fn count_lock_packages(lockfile: &Path) -> u64 {
    std::fs::read_to_string(lockfile).map_or(0, |lock| {
        u64::try_from(lock.lines().filter(|line| *line == "[[package]]").count())
            .unwrap_or(u64::MAX)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_this_workspace() {
        let stats = stats();
        assert!(stats.lines > 0, "workspace has Rust sources");
        assert!(stats.dependencies > 0, "Cargo.lock lists packages");
    }
}
