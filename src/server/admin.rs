//! Administrative endpoints: export, import, backup, sync.
//!
//! All require a bearer token (they're destructive / reach the network).

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::git;
use crate::server::error::ApiResult;
use crate::server::{rebuild_db_from_yaml, AppState};
use crate::yaml::Item;

const REMOTE: &str = "origin";
const BRANCH: &str = "master";

/// Dump everything in the DB to YAML files in the repo dir, removing any
/// stale YAML that has no DB counterpart. Commits the result.
async fn export(State(state): State<AppState>) -> ApiResult<Json<ExportReport>> {
    let (notes, pulses, metrics) = {
        let conn = state.db();
        (
            crate::db::list_notes(&conn, None)?,
            crate::db::list_pulses(&conn)?,
            crate::db::list_metrics(&conn)?,
        )
    };
    let repo = state.inner.paths.repo_dir.clone();
    // Clear the repo dir of .yaml files first so stale ones don't survive.
    let mut kept: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in std::fs::read_dir(&repo)? {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            std::fs::remove_file(&path).ok();
        }
    }
    for n in &notes {
        let _ = crate::yaml::write_item(&repo, &Item::Note(n.clone()))?;
        kept.insert(format!("{}.yaml", n.id));
    }
    for p in &pulses {
        let _ = crate::yaml::write_item(&repo, &Item::Pulse(p.clone()))?;
        kept.insert(format!("{}.yaml", p.id));
    }
    for m in &metrics {
        let _ = crate::yaml::write_item(&repo, &Item::Metric(m.clone()))?;
        kept.insert(format!("{}.yaml", m.id));
    }
    let mut paths: Vec<String> = kept.iter().cloned().collect();
    paths.sort();
    let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let committed = git::add_and_commit(&repo, &path_refs, "export: full rewrite")?;
    Ok(Json(ExportReport {
        notes: notes.len(),
        pulses: pulses.len(),
        metrics: metrics.len(),
        committed,
    }))
}

/// Reload the DB from the YAML files in the repo dir. Use after editing YAML
/// by hand or after a `sync` (pull).
async fn import(State(state): State<AppState>) -> ApiResult<Json<ImportReport>> {
    let repo = state.inner.paths.repo_dir.clone();
    let count = {
        let conn = state.db();
        rebuild_db_from_yaml(&conn, &repo)?
    };
    Ok(Json(ImportReport { items: count }))
}

/// `git push origin master`. The remote must already be configured (via
/// `git -C <repo> remote add origin <url>`).
async fn backup(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let repo = state.inner.paths.repo_dir.clone();
    git::push(&repo, REMOTE, BRANCH)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `git pull --ff-only origin master`, then rebuild the DB from the YAML
/// files that changed.
async fn sync(State(state): State<AppState>) -> ApiResult<Json<SyncReport>> {
    let repo = state.inner.paths.repo_dir.clone();
    let changed = git::pull(&repo, REMOTE, BRANCH)?;
    let items = {
        let conn = state.db();
        rebuild_db_from_yaml(&conn, &repo)?
    };
    Ok(Json(SyncReport {
        changed_files: changed
            .iter()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
            .collect(),
        items_loaded: items,
    }))
}

#[derive(Serialize)]
pub struct ExportReport {
    pub notes: usize,
    pub pulses: usize,
    pub metrics: usize,
    pub committed: bool,
}

#[derive(Serialize)]
pub struct ImportReport {
    pub items: usize,
}

#[derive(Serialize)]
pub struct SyncReport {
    pub changed_files: Vec<String>,
    pub items_loaded: usize,
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/export", routing::post(export))
        .route("/api/import", routing::post(import))
        .route("/api/backup", routing::post(backup))
        .route("/api/sync", routing::post(sync))
}
