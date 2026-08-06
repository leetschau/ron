//! Spawn `$EDITOR` (or `nvim`) on a temp file and return the final contents.

use std::io::Write;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

/// Open `initial` content in the user's editor. Returns what the buffer
/// contained when the editor exited.
pub fn edit(initial: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    let mut tmp = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .context("create temp file")?;
    tmp.write_all(initial.as_bytes())?;
    tmp.flush()?;
    let path = tmp.path().to_path_buf();
    // We need to keep the file on disk while the editor runs, then read it
    // back. Drop the handle at the end.
    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("spawning editor {editor}"))?;
    if !status.success() {
        return Err(anyhow!("editor {editor} exited with {status}"));
    }
    let text = std::fs::read_to_string(&path)?;
    Ok(text)
}
