//! Drafts: the note-edit recovery cache.
//!
//! Two surfaces over the same `drafts` table:
//!
//! * `/api/drafts...` — bearer-auth'd REST used by the CLI (`ron draft`,
//!   and the add/edit flows' prefill / push).
//! * `/drafts/:key...` — viewer-side endpoints for the browser form JS
//!   (autosave + explicit "save draft" button) and the discard buttons.
//!   Mounted under the `require_viewer` cookie gate like the forms.
//!
//! `updated` timestamps are stamped by the server so ordering between
//! drafts, notes, and watermarks is consistent regardless of client clocks.

use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use chrono::Local;
use serde::Deserialize;
use serde_json::json;

use crate::db;
use crate::models::{valid_draft_key, Draft, DraftContent};
use crate::server::error::{ApiError, ApiResult};
use crate::server::AppState;

/// Response of `GET /api/drafts/:key`: the live draft (if any) plus the
/// consumed watermark, so clients can drop stale local copies.
#[derive(Debug, serde::Serialize)]
pub struct DraftResponse {
    pub draft: Option<Draft>,
    pub consumed_updated: Option<chrono::NaiveDateTime>,
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Draft>>> {
    let drafts = {
        let conn = state.db();
        db::list_drafts(&conn)?
    };
    Ok(Json(drafts))
}

async fn get(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<Json<DraftResponse>> {
    if !valid_draft_key(&key) {
        return Err(ApiError::BadRequest("invalid draft key".into()));
    }
    let conn = state.db();
    let draft = db::get_draft(&conn, &key)?;
    let consumed_updated = db::watermark_for(&conn, &key)?;
    Ok(Json(DraftResponse {
        draft,
        consumed_updated,
    }))
}

/// Shared upsert: validate the key, stamp `updated`, return the stored draft.
fn upsert_inner(state: &AppState, key: &str, content: DraftContent) -> ApiResult<Draft> {
    if !valid_draft_key(key) {
        return Err(ApiError::BadRequest("invalid draft key".into()));
    }
    let now = Local::now().naive_local();
    let draft = Draft {
        key: key.to_string(),
        content,
        updated: now,
    };
    {
        let conn = state.db();
        db::upsert_draft(&conn, key, &draft.content, now)?;
    }
    Ok(draft)
}

async fn put(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(content): Json<DraftContent>,
) -> ApiResult<Json<Draft>> {
    Ok(Json(upsert_inner(&state, &key, content)?))
}

async fn delete(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    if !valid_draft_key(&key) {
        return Err(ApiError::BadRequest("invalid draft key".into()));
    }
    let removed = {
        let conn = state.db();
        db::delete_draft(&conn, &key)?
    };
    Ok(Json(json!({ "ok": removed })))
}

// ----- viewer-facing endpoints (cookie-gated via app.rs) ----------------------

/// `POST /drafts/:key` — autosave / "save draft" button. JSON in, JSON out.
async fn save_post(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(content): Json<DraftContent>,
) -> ApiResult<Json<serde_json::Value>> {
    let draft = upsert_inner(&state, &key, content)?;
    Ok(Json(json!({ "ok": true, "updated": draft.updated })))
}

#[derive(Debug, Deserialize)]
pub struct DiscardForm {
    /// Where to redirect after discarding; must start with `/`.
    #[serde(default)]
    pub back: Option<String>,
}

/// `POST /drafts/:key/discard` — drop the draft entirely (watermark
/// included) and redirect back.
pub async fn discard_post(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Form(form): Form<DiscardForm>,
) -> ApiResult<Response> {
    if !valid_draft_key(&key) {
        return Err(ApiError::BadRequest("invalid draft key".into()));
    }
    {
        let conn = state.db();
        db::delete_draft(&conn, &key)?;
    }
    let back = form
        .back
        .filter(|b| b.starts_with('/') && !b.starts_with("//"))
        .unwrap_or_else(|| "/".to_string());
    Ok(Redirect::to(&back).into_response())
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/drafts", routing::get(list))
        .route(
            "/api/drafts/:key",
            routing::get(get).put(put).delete(delete),
        )
}

pub fn viewer_routes() -> axum::Router<AppState> {
    use axum::routing::post;
    axum::Router::new()
        .route("/drafts/:key", post(save_post))
        .route("/drafts/:key/discard", post(discard_post))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("data");
        let cfg = dir.path().join("config");
        std::fs::create_dir_all(app.join("repo")).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();
        AppState::new(
            crate::paths::Paths {
                db_path: app.join("db.sqlite3"),
                repo_dir: app.join("repo"),
                server_config: cfg.join("server.json"),
                tokens_file: cfg.join("tokens.json"),
                app_home: app,
                config_dir: cfg,
            },
            &crate::paths::ServerConfig::default(),
        )
        .unwrap()
    }

    fn content() -> DraftContent {
        DraftContent {
            title: "wip".into(),
            tags: vec!["x".into()],
            notebook: "default".into(),
            related: vec![],
            body: "draft body".into(),
        }
    }

    #[tokio::test]
    async fn draft_upsert_get_delete_roundtrip() {
        let state = test_state();
        let stored = upsert_inner(&state, "new", content()).unwrap();
        assert_eq!(stored.content.title, "wip");

        let conn = state.db();
        assert_eq!(db::get_draft(&conn, "new").unwrap().unwrap().content.body, "draft body");
        db::consume_draft(&conn, "new", chrono::Local::now().naive_local()).unwrap();
        assert!(db::get_draft(&conn, "new").unwrap().is_none());
        assert!(db::watermark_for(&conn, "new").unwrap().is_some());
        assert!(db::delete_draft(&conn, "new").unwrap());
        assert!(db::get_draft(&conn, "new").unwrap().is_none());
    }

    #[tokio::test]
    async fn draft_upsert_rejects_bad_key() {
        let state = test_state();
        let err = upsert_inner(&state, "note:../etc", content()).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}
