//! Thin shell-out wrappers around the `git` CLI.
//!
//! The server owns a git repo at `<app_home>/repo`. YAML files written there
//! on every API write are immediately `git add`-ed and committed, so the
//! commit history mirrors the dataset's evolution. `backup`/`sync` push/pull
//! from the configured remote.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

const DEFAULT_BRANCH: &str = "master";

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

/// Configure a remote URL. Idempotent.
pub fn set_remote(repo: &Path, name: &str, url: &str) -> Result<()> {
    let exists = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["remote", "get-url", name])
        .output()?;
    if !exists.status.success() {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["remote", "add", name, url])
            .output()?;
        if !out.status.success() {
            anyhow::bail!(
                "git remote add failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    } else {
        let current = String::from_utf8_lossy(&exists.stdout).trim().to_string();
        if current != url {
            let out = Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["remote", "set-url", name, url])
                .output()?;
            if !out.status.success() {
                anyhow::bail!(
                    "git remote set-url failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
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
}
