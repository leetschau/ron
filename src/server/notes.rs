//! REST API for Notes.

use std::path::PathBuf;

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
    if body.title.trim().is_empty() {
        return Err(ApiError::BadRequest("title must not be empty".into()));
    }
    let now = Local::now().naive_local();
    let note = Note {
        id: new_id(Kind::Note),
        title: body.title,
        tags: body.tags,
        notebook: if body.notebook.is_empty() {
            "default".into()
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
    persist_yaml(&state, yaml::Item::Note(note.clone()))?;
    Ok((axum::http::StatusCode::CREATED, Json(note)))
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
    let mut note = {
        let conn = state.db();
        db::get_note(&conn, &id)?.ok_or(ApiError::NotFound)?
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
    persist_yaml(&state, yaml::Item::Note(note.clone()))?;
    Ok(Json(note))
}

async fn delete(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let removed = {
        let conn = state.db();
        db::delete_note(&conn, &id)?
    };
    if !removed {
        return Err(ApiError::NotFound);
    }
    delete_yaml(&state, &id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Write a single item's YAML file in the repo dir.
pub fn persist_yaml(state: &AppState, item: yaml::Item) -> ApiResult<()> {
    if let Err(e) = yaml::write_item(&state.inner.paths.repo_dir, &item) {
        eprintln!("warning: yaml write failed: {e:#}");
    }
    Ok(())
}

/// Remove a single item's YAML file by id.
pub fn delete_yaml(state: &AppState, id: &str) -> ApiResult<()> {
    let path: PathBuf = state.inner.paths.repo_dir.join(format!("{id}.yaml"));
    if path.exists() {
        if let Err(e) = std::fs::remove_file(&path) {
            eprintln!("warning: yaml delete failed {}: {e}", path.display());
        }
    }
    Ok(())
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/notes", routing::get(list).post(create))
        .route("/api/notes/search", routing::get(search))
        .route("/api/notes/:id", routing::get(get).put(update).delete(delete))
}
