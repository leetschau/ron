//! Thin shell-out wrappers around the `git` CLI.
//!
//! The server owns a git repo at `<app_home>/repo`. YAML files written there
//! on every API write are immediately `git add`-ed and committed, so the
//! commit history mirrors the dataset's evolution. `backup`/`sync` push/pull
//! from the configured remote.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_BRANCH: &str = "master";

/// Cap on how many commits `BackupStatus` lists per direction.
const MAX_STATUS_LOG: usize = 20;

/// Ensure `repo` is a git repo. Idempotent.
pub fn ensure_repo(repo: &Path) -> Result<()> {
    if !repo.join(".git").exists() {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["init", "-b", DEFAULT_BRANCH])
            .output()
            .context("git init")?;
        if !out.status.success() {
            anyhow::bail!(
                "git init failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
    Ok(())
}

/// Remote URL, or `None` when the remote isn't configured.
pub fn remote_url(repo: &Path, name: &str) -> Result<Option<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "get-url", name])
        .output()?;
    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    ))
}

/// Configure a remote URL. Idempotent.
pub fn set_remote(repo: &Path, name: &str, url: &str) -> Result<()> {
    let current = remote_url(repo, name)?;
    let out = match current {
        None => Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["remote", "add", name, url])
            .output()?,
        Some(cur) if cur != url => Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["remote", "set-url", name, url])
            .output()?,
        // Already set to this URL.
        Some(_) => return Ok(()),
    };
    if !out.status.success() {
        anyhow::bail!(
            "git remote add/set-url failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Stage `paths` and commit with `message`. Returns true if a commit was
/// created, false if the working tree was clean.
pub fn add_and_commit(repo: &Path, paths: &[&str], message: &str) -> Result<bool> {
    // Stage
    let mut add = Command::new("git");
    add.arg("-C").arg(repo).arg("add").arg("--").args(paths);
    let out = add.output().context("git add")?;
    if !out.status.success() {
        anyhow::bail!(
            "git add failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    // Commit (may be a no-op).
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", message])
        .output()
        .context("git commit")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // `nothing to commit, working tree clean` is the common benign case.
        if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
            return Ok(false);
        }
        anyhow::bail!("git commit failed: {}", stderr.trim());
    }
    Ok(true)
}

/// Remove a path (file) from the index and commit its deletion.
pub fn remove_and_commit(repo: &Path, paths: &[&str], message: &str) -> Result<bool> {
    let mut rm = Command::new("git");
    rm.arg("-C")
        .arg(repo)
        .args(["rm", "--quiet", "--"])
        .args(paths);
    let out = rm.output().context("git rm")?;
    if !out.status.success() {
        // Path may not be tracked yet (e.g. never committed). That's fine.
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("did not match any files") {
            anyhow::bail!("git rm failed: {}", stderr.trim());
        }
        return Ok(false);
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", message])
        .output()?;
    Ok(out.status.success())
}

/// Like [`add_and_commit`], but stages the whole working tree first (`git
/// add -A`) so deletions and renames are committed alongside the given
/// paths. Used by export, which rewrites the YAML layout wholesale.
pub fn add_all_and_commit(repo: &Path, _paths: &[&str], message: &str) -> Result<bool> {
    let mut add = Command::new("git");
    add.arg("-C").arg(repo).arg("add").arg("-A");
    let out = add.output().context("git add -A")?;
    if !out.status.success() {
        anyhow::bail!(
            "git add -A failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", message])
        .output()
        .context("git commit")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
            return Ok(false);
        }
        anyhow::bail!("git commit failed: {}", stderr.trim());
    }
    Ok(true)
}

pub fn push(repo: &Path, remote: &str, branch: &str) -> Result<()> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["push", remote, branch])
        .output()
        .with_context(|| format!("git push {remote} {branch}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git push failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Pull and return the list of YAML files that changed.
pub fn pull(repo: &Path, remote: &str, branch: &str) -> Result<Vec<PathBuf>> {
    let before = head_files(repo)?;
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["pull", "--ff-only", remote, branch])
        .output()
        .with_context(|| format!("git pull {remote} {branch}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git pull failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let after = head_files(repo)?;
    let mut diff: Vec<PathBuf> = after
        .iter()
        .filter(|p| !before.contains(p))
        .cloned()
        .collect();
    diff.extend(before.iter().filter(|p| !after.contains(p)).cloned());
    Ok(diff)
}

/// List all files in HEAD (only tracked paths).
pub fn head_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-tree", "-r", "--name-only", "HEAD"])
        .output()?;
    if !out.status.success() {
        // Repo may have no commits yet.
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| repo.join(s))
        .collect())
}

/// One line of `git log --oneline`: short hash + subject.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitLine {
    pub hash: String,
    pub subject: String,
}

/// Backup/sync status snapshot (`ron backup --dry-run`).
#[derive(Debug, Serialize, Deserialize)]
pub struct BackupStatus {
    /// Remote URL; `None` means no remote is configured.
    pub remote_url: Option<String>,
    /// Whether the pre-check `git fetch` reached the remote. When false the
    /// ahead/behind counts compare against a possibly-stale tracking ref.
    pub fetched: bool,
    /// Local commits not on `<remote>/<branch>`.
    pub ahead: usize,
    /// Remote commits not on the local branch.
    pub behind: usize,
    /// Whether the working tree / index has uncommitted changes.
    pub dirty: bool,
    /// Unpushed local commits, newest first (capped).
    pub to_push: Vec<CommitLine>,
    /// Unpulled remote commits, newest first (capped).
    pub to_pull: Vec<CommitLine>,
}

impl BackupStatus {
    pub fn diverged(&self) -> bool {
        self.ahead > 0 && self.behind > 0
    }
}

/// Fetch the remote (updates its remote-tracking refs) without merging.
/// Returns false when the remote is unreachable (offline) — not an error,
/// the status just goes stale.
fn fetch(repo: &Path, remote: &str) -> bool {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["fetch", remote])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// Whether the remote-tracking ref `<remote>/<branch>` exists locally.
fn has_remote_tracking(repo: &Path, remote: &str, branch: &str) -> bool {
    rev_exists(repo, &format!("{remote}/{branch}"))
}

/// Whether a rev (branch, tracking ref, ...) resolves.
fn rev_exists(repo: &Path, rev: &str) -> bool {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--verify", "--quiet", rev])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

/// Commits on `branch` (0 for an unborn branch).
fn count_commits(repo: &Path, branch: &str) -> Result<usize> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-list", "--count", branch])
        .output()?;
    if !out.status.success() {
        return Ok(0);
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(0))
}

/// (ahead, behind) of `branch` vs `<remote>/<branch>`; assumes the tracking
/// ref exists.
fn ahead_behind(repo: &Path, remote: &str, branch: &str) -> Result<(usize, usize)> {
    let range = format!("{branch}...{remote}/{branch}");
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-list", "--count", "--left-right", &range])
        .output()?;
    if !out.status.success() {
        return Ok((0, 0));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut it = text.split_whitespace();
    let ahead = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let behind = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok((ahead, behind))
}

/// One-line log entries for a rev range (or a single rev), newest first,
/// capped at `max`. Empty when the revs don't resolve.
fn log_oneline(repo: &Path, revspec: &str, max: usize) -> Vec<CommitLine> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "--format=%h %s", "-n"])
        .arg(max.to_string())
        .arg(revspec)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter_map(|l| l.split_once(' '))
            .map(|(hash, subject)| CommitLine {
                hash: hash.to_string(),
                subject: subject.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// True when tracked files have uncommitted changes (modified/staged/
/// deleted). Untracked files are ignored on purpose: the server leaves its
/// auto-created `.gitignore` and hand-dropped `resources/` untracked, which
/// is benign; only tracked drift means DB and repo are out of sync.
pub fn is_dirty(repo: &Path) -> Result<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()?;
    Ok(out.status.success()
        && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Gather the backup/sync status: remote config, uncommitted changes, and
/// ahead/behind vs `<remote>/<branch>` after a best-effort fetch. Never
/// fails on an unreachable remote — the counts are just reported as stale.
pub fn backup_status(repo: &Path, remote: &str, branch: &str) -> Result<BackupStatus> {
    let remote_url = remote_url(repo, remote)?;
    let dirty = is_dirty(repo)?;
    let fetched = remote_url.is_some() && fetch(repo, remote);
    let tracking = format!("{remote}/{branch}");
    let (ahead, behind, to_push, to_pull) = match (
        rev_exists(repo, branch),
        has_remote_tracking(repo, remote, branch),
    ) {
        (true, true) => {
            let (a, b) = ahead_behind(repo, remote, branch)?;
            (
                a,
                b,
                log_oneline(repo, &format!("{tracking}..{branch}"), MAX_STATUS_LOG),
                log_oneline(repo, &format!("{branch}..{tracking}"), MAX_STATUS_LOG),
            )
        }
        // Unborn local branch: everything on the remote is to pull.
        (false, true) => (
            0,
            count_commits(repo, &tracking)?,
            Vec::new(),
            log_oneline(repo, &tracking, MAX_STATUS_LOG),
        ),
        // No tracking ref yet (nothing fetched/pushed): every local commit
        // is unpushed.
        (has_local, _) => (
            if has_local { count_commits(repo, branch)? } else { 0 },
            0,
            if has_local {
                log_oneline(repo, branch, MAX_STATUS_LOG)
            } else {
                Vec::new()
            },
            Vec::new(),
        ),
    };
    Ok(BackupStatus {
        remote_url,
        fetched,
        ahead,
        behind,
        dirty,
        to_push,
        to_pull,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn config(repo: &Path) {
        for (k, v) in [("user.name", "ron"), ("user.email", "ron@localhost")] {
            let _ = Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["config", k, v])
                .status();
        }
    }

    #[test]
    fn ensure_repo_creates_and_is_idempotent() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        ensure_repo(&repo).unwrap();
        ensure_repo(&repo).unwrap();
        assert!(repo.join(".git").exists());
    }

    #[test]
    fn remote_url_none_then_set_and_reset() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        ensure_repo(repo).unwrap();
        assert_eq!(remote_url(repo, "origin").unwrap(), None);
        set_remote(repo, "origin", "https://example.com/r.git").unwrap();
        assert_eq!(
            remote_url(repo, "origin").unwrap().as_deref(),
            Some("https://example.com/r.git")
        );
        // Same URL again is a no-op; a new URL is written through.
        set_remote(repo, "origin", "https://example.com/r.git").unwrap();
        set_remote(repo, "origin", "https://example.com/r2.git").unwrap();
        assert_eq!(
            remote_url(repo, "origin").unwrap().as_deref(),
            Some("https://example.com/r2.git")
        );
    }

    #[test]
    fn add_and_commit_returns_true_on_change() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        ensure_repo(repo).unwrap();
        config(repo);
        std::fs::write(repo.join("a.yaml"), "hello").unwrap();
        assert!(add_and_commit(repo, &["a.yaml"], "first").unwrap());
        // Second commit with no change -> false.
        assert!(!add_and_commit(repo, &["a.yaml"], "second").unwrap());
    }

    #[test]
    fn rm_and_commit_works() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        ensure_repo(repo).unwrap();
        config(repo);
        std::fs::write(repo.join("b.yaml"), "x").unwrap();
        add_and_commit(repo, &["b.yaml"], "add").unwrap();
        std::fs::remove_file(repo.join("b.yaml")).unwrap();
        assert!(remove_and_commit(repo, &["b.yaml"], "gone").unwrap());
    }

    #[test]
    fn head_files_lists_tracked() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        ensure_repo(repo).unwrap();
        config(repo);
        std::fs::write(repo.join("p.yaml"), "1").unwrap();
        std::fs::write(repo.join("q.yaml"), "2").unwrap();
        add_and_commit(repo, &["p.yaml", "q.yaml"], "x").unwrap();
        let files = head_files(repo).unwrap();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"p.yaml".into()));
        assert!(names.contains(&"q.yaml".into()));
    }

    /// Bare repo to use as `origin`, plus a working clone wired to it.
    fn origin_and_clone(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
        let origin = dir.join("origin.git");
        let out = Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg("-b")
            .arg(DEFAULT_BRANCH)
            .arg(&origin)
            .output()
            .unwrap();
        assert!(out.status.success(), "git init --bare failed");
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        ensure_repo(&work).unwrap();
        config(&work);
        set_remote(&work, "origin", origin.to_str().unwrap()).unwrap();
        (origin, work)
    }

    #[test]
    fn backup_status_ahead_then_pushed() {
        let dir = tempdir().unwrap();
        let (origin, work) = origin_and_clone(dir.path());

        // Fresh clone: no commits, no remote tracking ref yet.
        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!(st.remote_url.as_deref(), Some(origin.to_str().unwrap()));
        assert!(st.fetched);
        assert_eq!((st.ahead, st.behind), (0, 0));
        assert!(st.to_push.is_empty());

        // Local commit -> ahead 1 with the commit listed.
        std::fs::write(work.join("a.yaml"), "1").unwrap();
        add_and_commit(&work, &["a.yaml"], "first").unwrap();
        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!((st.ahead, st.behind), (1, 0));
        assert_eq!(st.to_push.len(), 1);
        assert_eq!(st.to_push[0].subject, "first");
        assert!(st.to_pull.is_empty());
        assert!(!st.diverged());

        // After push everything is in sync.
        push(&work, "origin", DEFAULT_BRANCH).unwrap();
        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!((st.ahead, st.behind), (0, 0));
    }

    #[test]
    fn backup_status_behind_and_diverged() {
        let dir = tempdir().unwrap();
        let (origin, work) = origin_and_clone(dir.path());

        // A second clone pushes; `work` falls behind by 1.
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        ensure_repo(&other).unwrap();
        config(&other);
        set_remote(&other, "origin", origin.to_str().unwrap()).unwrap();
        std::fs::write(other.join("o.yaml"), "1").unwrap();
        add_and_commit(&other, &["o.yaml"], "remote commit").unwrap();
        push(&other, "origin", DEFAULT_BRANCH).unwrap();

        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!((st.ahead, st.behind), (0, 1));
        assert_eq!(st.to_pull.len(), 1);
        assert_eq!(st.to_pull[0].subject, "remote commit");
        assert!(!st.diverged());

        // Commit locally too -> diverged.
        std::fs::write(work.join("w.yaml"), "2").unwrap();
        add_and_commit(&work, &["w.yaml"], "local commit").unwrap();
        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!((st.ahead, st.behind), (1, 1));
        assert!(st.diverged());
    }

    #[test]
    fn backup_status_flags_dirty_tree() {
        let dir = tempdir().unwrap();
        let repo = dir.path();
        ensure_repo(repo).unwrap();
        config(repo);
        std::fs::write(repo.join("a.yaml"), "1").unwrap();
        add_and_commit(repo, &["a.yaml"], "x").unwrap();

        // Untracked files alone (like the auto-created .gitignore) are
        // benign and must not read as dirty.
        std::fs::write(repo.join(".gitignore"), "*.sqlite*").unwrap();
        assert!(!is_dirty(repo).unwrap());

        // A modification to a tracked file is dirty.
        std::fs::write(repo.join("a.yaml"), "changed").unwrap();
        assert!(is_dirty(repo).unwrap());

        let (origin, _) = origin_and_clone(dir.path());
        set_remote(repo, "origin", origin.to_str().unwrap()).unwrap();
        let st = backup_status(repo, "origin", DEFAULT_BRANCH).unwrap();
        assert!(st.dirty);
    }

    #[test]
    fn backup_status_survives_unreachable_remote() {
        let dir = tempdir().unwrap();
        let (origin, work) = origin_and_clone(dir.path());
        std::fs::write(work.join("a.yaml"), "1").unwrap();
        add_and_commit(&work, &["a.yaml"], "first").unwrap();
        push(&work, "origin", DEFAULT_BRANCH).unwrap();
        std::fs::write(work.join("b.yaml"), "2").unwrap();
        add_and_commit(&work, &["b.yaml"], "second").unwrap();

        // Remote gone (e.g. USB drive removed): status still works, just
        // marked stale.
        std::fs::remove_dir_all(&origin).unwrap();
        let st = backup_status(&work, "origin", DEFAULT_BRANCH).unwrap();
        assert_eq!(st.remote_url.as_deref(), Some(origin.to_str().unwrap()));
        assert!(!st.fetched);
        // Counts vs the stale tracking ref: second commit is ahead.
        assert_eq!((st.ahead, st.behind), (1, 0));
    }
}
