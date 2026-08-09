//! HTTP server: REST API for notes/pulses/metrics + browser viewer.

pub mod admin;
pub mod app;
pub mod auth;
pub mod error;
pub mod metrics;
pub mod notes;
pub mod pulses;
pub mod tokens;

use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::paths::Paths;
use crate::token::TokenStore;

/// Shared application state. Cheaply cloneable via Arc.
#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<Inner>,
}

pub struct Inner {
    pub paths: Paths,
    pub db: std::sync::Mutex<Connection>,
    pub tokens: std::sync::RwLock<TokenStore>,
}

impl AppState {
    pub fn new(paths: Paths) -> Result<Self> {
        // Ensure the git repo exists before opening the DB; .gitignore below
        // keeps the SQLite store (which lives *outside* the repo anyway) from
        // being tracked if it's ever moved in.
        crate::git::ensure_repo(&paths.repo_dir)?;
        write_gitignore(&paths.repo_dir)?;
        let conn = crate::db::open(&paths.db_path)
            .with_context(|| format!("opening db {}", paths.db_path.display()))?;
        // Bootstrap the DB from YAML if it appears empty (cold start / sync
        // from another machine).
        bootstrap_from_yaml(&conn, &paths.repo_dir)?;
        Ok(Self {
            inner: Arc::new(Inner {
                paths,
                db: std::sync::Mutex::new(conn),
                tokens: std::sync::RwLock::new(TokenStore::default()),
            }),
        })
    }

    pub fn load_tokens(&self) -> Result<()> {
        let store = TokenStore::load(&self.inner.paths.tokens_file)?;
        *self.inner.tokens.write().unwrap() = store;
        Ok(())
    }

    pub fn save_tokens(&self) -> Result<()> {
        let store = self.inner.tokens.read().unwrap().clone();
        store.save(&self.inner.paths.tokens_file)
    }

    /// Lock the DB connection. Panics if poisoned.
    pub fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.inner.db.lock().unwrap()
    }
}

/// Add a `.gitignore` that excludes the SQLite store and other transient
/// files if they ever end up under the repo dir.
fn write_gitignore(repo_dir: &std::path::Path) -> Result<()> {
    let path = repo_dir.join(".gitignore");
    const CONTENT: &str = "*.sqlite*\n*.db*\n.wal\n.shm\n";
    if !path.exists() {
        std::fs::write(&path, CONTENT).ok();
    }
    Ok(())
}

/// If all data tables are empty and there are YAML files in the repo dir,
/// load them into the DB. This is the cold-start / sync-from-another-machine
/// path.
pub fn bootstrap_from_yaml(conn: &Connection, repo_dir: &std::path::Path) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))?;
    let pulse_count: i64 = conn.query_row("SELECT COUNT(*) FROM pulses", [], |r| r.get(0))?;
    let metric_count: i64 = conn.query_row("SELECT COUNT(*) FROM metrics", [], |r| r.get(0))?;
    if count + pulse_count + metric_count > 0 {
        return Ok(());
    }
    if !repo_dir.exists() {
        return Ok(());
    }
    let items = crate::yaml::read_all(repo_dir).unwrap_or_default();
    if items.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    for item in items {
        match item {
            crate::yaml::Item::Note(n) => crate::db::upsert_note(&tx, &n)?,
            crate::yaml::Item::Pulse(p) => crate::db::upsert_pulse(&tx, &p)?,
            crate::yaml::Item::Metric(m) => crate::db::upsert_metric(&tx, &m)?,
        }
    }
    tx.commit()?;
    Ok(())
}

/// Drop every row from every data table, then reload from YAML. Used by
/// `import` and `sync` after the YAML has been refreshed from disk.
pub fn rebuild_db_from_yaml(conn: &Connection, repo_dir: &std::path::Path) -> Result<usize> {
    let items = crate::yaml::read_all(repo_dir).unwrap_or_default();
    let n = items.len();
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM notes", [])?;
    tx.execute("DELETE FROM pulses", [])?;
    tx.execute("DELETE FROM metrics", [])?;
    for item in items {
        match item {
            crate::yaml::Item::Note(n) => crate::db::upsert_note(&tx, &n)?,
            crate::yaml::Item::Pulse(p) => crate::db::upsert_pulse(&tx, &p)?,
            crate::yaml::Item::Metric(m) => crate::db::upsert_metric(&tx, &m)?,
        }
    }
    tx.commit()?;
    Ok(n)
}