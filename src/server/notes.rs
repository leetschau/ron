//! REST API for Notes.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Local;
use serde::Deserialize;

use crate::db;
use crate::id::{new_id, Kind};
use crate::models::Note;
use crate::server::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::yaml;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub limit: Option<u32>,
}

async fn list(State(state): State<AppState>, Query(p): Query<ListParams>) -> ApiResult<Json<Vec<Note>>> {
    let notes = {
        let conn = state.db();
        db::list_notes(&conn, p.limit)?
    };
    Ok(Json(notes))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default)]
    pub field: SearchField,
    #[serde(default = "default_true")]
    pub ignore_case: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SearchField {
    #[default]
    Content,
    Title,
    Tags,
    Notebook,
}

impl From<SearchField> for db::NoteField {
    fn from(f: SearchField) -> Self {
        match f {
            SearchField::Content => db::NoteField::Content,
            SearchField::Title => db::NoteField::Title,
            SearchField::Tags => db::NoteField::Tags,
            SearchField::Notebook => db::NoteField::Notebook,
        }
    }
}

async fn search(
    State(state): State<AppState>,
    Query(p): Query<SearchParams>,
) -> ApiResult<Json<Vec<Note>>> {
    let notes = {
        let conn = state.db();
        db::search_notes(
            &conn,
            p.field.into(),
            &p.q,
            db::NoteMatch {
                ignore_case: p.ignore_case,
                whole_word: p.whole_word,
                from: p.from.as_deref().and_then(|s| db::parse_when(s, false)),
                to: p.to.as_deref().and_then(|s| db::parse_when(s, true)),
                order_by: None,
            },
        )?
    };
    Ok(Json(notes))
}

async fn get(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Note>> {
    let conn = state.db();
    db::get_note(&conn, &id)?
        .ok_or(ApiError::NotFound)
        .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notebook: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub related: Vec<String>,
}

async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(axum::http::StatusCode, Json<Note>)> {
    let note = create_note_inner(&state, body).await?;
    Ok((axum::http::StatusCode::CREATED, Json(note)))
}

/// Shared create logic (used by the JSON API and the viewer's form POST).
pub async fn create_note_inner(state: &AppState, body: CreateBody) -> ApiResult<Note> {
    if body.title.trim().is_empty() {
        return Err(ApiError::BadRequest("title must not be empty".into()));
    }
    let now = Local::now().naive_local();
    let note = Note {
        id: new_id(Kind::Note),
        title: body.title,
        tags: body.tags,
        notebook: if body.notebook.is_empty() {
            state.inner.default_notebook.clone()
        } else {
            body.notebook
        },
        created: now,
        updated: now,
        related: body.related,
        body: body.body,
    };
    {
        let conn = state.db();
        db::upsert_note(&conn, &note)?;
    }
    persist_yaml(state, yaml::Item::Note(note.clone()))?;
    consume_draft_best_effort(state, "new");
    Ok(note)
}

#[derive(Debug, Deserialize)]
pub struct UpdateBody {
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub notebook: Option<String>,
    pub body: Option<String>,
    /// Replace the related list. Use `None` to leave untouched; pass `[]` to clear.
    pub related: Option<Vec<String>>,
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Note>> {
    let note = update_note_inner(&state, &id, body).await?;
    Ok(Json(note))
}

/// Shared update logic (used by the JSON API and the viewer's edit form).
pub async fn update_note_inner(
    state: &AppState,
    id: &str,
    body: UpdateBody,
) -> ApiResult<Note> {
    let mut note = {
        let conn = state.db();
        db::get_note(&conn, id)?.ok_or(ApiError::NotFound)?
    };
    if let Some(t) = body.title {
        if t.trim().is_empty() {
            return Err(ApiError::BadRequest("title must not be empty".into()));
        }
        note.title = t;
    }
    if let Some(t) = body.tags {
        note.tags = t;
    }
    if let Some(n) = body.notebook {
        note.notebook = n;
    }
    if let Some(b) = body.body {
        note.body = b;
    }
    if let Some(r) = body.related {
        note.related = r;
    }
    note.updated = Local::now().naive_local();
    {
        let conn = state.db();
        db::upsert_note(&conn, &note)?;
    }
    persist_yaml(state, yaml::Item::Note(note.clone()))?;
    consume_draft_best_effort(state, &format!("note:{id}"));
    Ok(note)
}

/// Mark the draft for `key` consumed (its note was saved). The note write
/// already succeeded, so a failure here is only logged — the draft would
/// just linger until `ron draft clear`.
fn consume_draft_best_effort(state: &AppState, key: &str) {
    let res = {
        let conn = state.db();
        db::consume_draft(&conn, key, Local::now().naive_local())
    };
    if let Err(e) = res {
        eprintln!("warning: draft consume failed for {key}: {e}");
    }
}

async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let removed = delete_note_inner(&state, &id).await?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Shared delete logic (used by the JSON API and the viewer's delete form).
/// Returns whether anything was removed.
pub async fn delete_note_inner(state: &AppState, id: &str) -> ApiResult<bool> {
    let removed = {
        let conn = state.db();
        db::delete_note(&conn, id)?
    };
    if removed {
        delete_yaml(state, id)?;
    }
    Ok(removed)
}

/// Write a single item's YAML file in the repo dir, then commit it.
pub fn persist_yaml(state: &AppState, item: yaml::Item) -> ApiResult<()> {
    let path = match yaml::write_item(&state.inner.paths.repo_dir, &item) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("warning: yaml write failed: {e:#}");
            return Ok(());
        }
    };
    // Git path relative to the repo root (notes/<id>.yaml).
    let rel = path
        .strip_prefix(&state.inner.paths.repo_dir)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    let msg = match &item {
        yaml::Item::Note(n) => format!("note: {}: {}", n.id, summary(&n.title)),
        yaml::Item::Pulse(p) => format!("pulse: {}: {}", p.id, summary(&p.topic)),
        yaml::Item::Metric(m) => format!("metric: {}: {}", m.id, summary(&m.topic)),
    };
    if let Err(e) = crate::git::add_and_commit(&state.inner.paths.repo_dir, &[&rel], &msg) {
        eprintln!("warning: git commit failed: {e:#}");
    }
    Ok(())
}

/// Remove a single item's YAML file by id, then commit the deletion.
pub fn delete_yaml(state: &AppState, id: &str) -> ApiResult<()> {
    if let Some(rel) = yaml::rel_path(id) {
        let path = state.inner.paths.repo_dir.join(&rel);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                eprintln!("warning: yaml delete failed {}: {e}", path.display());
            }
        }
        if let Err(e) = crate::git::remove_and_commit(&state.inner.paths.repo_dir, &[&rel], &format!("delete: {id}")) {
            eprintln!("warning: git rm/commit failed: {e:#}");
        }
    }
    Ok(())
}

fn summary(s: &str) -> String {
    let t = s.trim();
    if t.chars().count() <= 60 {
        t.to_string()
    } else {
        let mut out: String = t.chars().take(59).collect();
        out.push('…');
        out
    }
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/notes", routing::get(list).post(create))
        .route("/api/notes/search", routing::get(search))
        .route("/api/notes/:id", routing::get(get).put(update).delete(delete))
}
