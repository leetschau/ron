//! Token management endpoints.
//!
//! These endpoints are reachable without a bearer token (see `auth.rs`),
//! relying on the server's localhost-only bind for security. They let the
//! local CLI mint tokens that browser sessions (or any other client) can use.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::server::error::{ApiError, ApiResult};
use crate::server::AppState;
use crate::token::TokenRecord;

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<TokenView>>> {
    let store = state.inner.tokens.read().unwrap();
    let view: Vec<TokenView> = store
        .list()
        .iter()
        .map(TokenView::from)
        .collect();
    Ok(Json(view))
}

#[derive(Debug, Deserialize)]
pub struct GrantBody {
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct GrantResponse {
    pub id: String,
    pub label: String,
    /// Only ever returned at grant time.
    pub secret: String,
}

async fn grant(
    State(state): State<AppState>,
    Json(body): Json<GrantBody>,
) -> ApiResult<Json<GrantResponse>> {
    let label = if body.label.trim().is_empty() {
        "unlabeled".to_string()
    } else {
        body.label
    };
    let (secret, record) = {
        let mut store = state.inner.tokens.write().unwrap();
        store.grant(label)
    };
    // Persist after releasing the write lock — save_tokens takes a read lock.
    state.save_tokens()?;
    Ok(Json(GrantResponse {
        id: record.id,
        label: record.label,
        secret,
    }))
}

async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = {
        let mut store = state.inner.tokens.write().unwrap();
        store.revoke(&id)
    };
    state.save_tokens()?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Serialize)]
pub struct TokenView {
    pub id: String,
    pub label: String,
    pub created: chrono::NaiveDateTime,
}

impl From<&TokenRecord> for TokenView {
    fn from(r: &TokenRecord) -> Self {
        TokenView {
            id: r.id.clone(),
            label: r.label.clone(),
            created: r.created,
        }
    }
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/tokens", routing::get(list).post(grant))
        .route("/api/tokens/:id", routing::delete(revoke))
}
