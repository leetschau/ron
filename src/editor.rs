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

/// Open `initial` content in the user's editor. Returns what the buffer
/// contained when the editor exited.
pub fn edit(initial: &str) -> Result<String> {
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
    if !status.success() {
        return Err(anyhow!("editor {prog} exited with {status}"));
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(text)
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
