//! Spawn the user's editor on a temp file and return the final contents.
//!
//! Editor resolution (first match wins): the `editor` key in
//! `~/.config/ron/server.json` (may include args, e.g. `code -w`), then
//! `$EDITOR`, then `nvim`.

use std::io::Write;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

/// Resolve the editor command line per the precedence above, split into
/// program + args on whitespace.
fn resolve_editor() -> Vec<String> {
    let editor = crate::paths::read_editor()
        .or_else(|| std::env::var("EDITOR").ok())
        .unwrap_or_else(|| "nvim".to_string());
    split_cmd(&editor)
}

/// Split a command line (program + args) on whitespace. Shared with the
/// `cli_viewer` runner in `main.rs`.
pub fn split_cmd(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

/// How the editor session ended. A non-zero exit (`:cq` in vim/nvim, or a
/// crash) still returns the buffer — ron treats it as "save as draft"
/// intent rather than discarding the user's typing.
#[derive(Debug, Clone)]
pub enum EditOutcome {
    /// Editor exited 0.
    Saved(String),
    /// Editor exited non-zero; the buffer as last written.
    ExitedNonzero(String),
}

impl EditOutcome {
    /// The buffer contents regardless of exit status.
    pub fn text(&self) -> &str {
        match self {
            EditOutcome::Saved(t) | EditOutcome::ExitedNonzero(t) => t,
        }
    }
}

/// Open `initial` content in the user's editor. Returns how the session
/// ended plus what the buffer contained. Spawning the editor at all still
/// fails hard (misconfigured `editor` key, binary not found).
pub fn edit(initial: &str) -> Result<EditOutcome> {
    let cmd = resolve_editor();
    let (prog, args) = cmd.split_first().ok_or_else(|| anyhow!("empty editor command"))?;
    let mut tmp = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .context("create temp file")?;
    tmp.write_all(initial.as_bytes())?;
    tmp.flush()?;
    let path = tmp.path().to_path_buf();
    // We need to keep the file on disk while the editor runs, then read it
    // back. Drop the handle at the end.
    let status = Command::new(prog)
        .args(args)
        .arg(&path)
        .status()
        .with_context(|| format!("spawning editor {prog}"))?;
    let text = std::fs::read_to_string(&path)?;
    if status.success() {
        Ok(EditOutcome::Saved(text))
    } else {
        Ok(EditOutcome::ExitedNonzero(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_cmd_handles_program_and_args() {
        assert_eq!(split_cmd("nvim"), vec!["nvim".to_string()]);
        assert_eq!(
            split_cmd("  code   --wait  "),
            vec!["code".to_string(), "--wait".to_string()]
        );
        assert!(split_cmd("   ").is_empty());
    }
}
