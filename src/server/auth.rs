//! Bearer-token authentication middleware.
//!
//! Skips the token endpoints themselves (`/api/tokens/*`) since those manage
//! the bootstrap and assume localhost-only access.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::server::AppState;

/// Extract a bearer token from the `Authorization: Bearer <token>` header.
pub fn extract_bearer(headers: &axum::http::HeaderMap) -> Option<String> {
    let value = headers.get(axum::http::header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    let token = s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer "))?;
    Some(token.trim().to_string())
}

pub async fn require_token(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // The token-management routes are exempt.
    let path = req.uri().path();
    if path == "/api/tokens" || path.starts_with("/api/tokens/") {
        return Ok(next.run(req).await);
    }
    // Browser-facing HTML routes are also exempt for P2. They are read-only
    // views over the local dataset; auth for them can be layered on later.
    if path == "/" || path.starts_with("/view/") || path.starts_with("/static/") {
        return Ok(next.run(req).await);
    }

    let Some(secret) = extract_bearer(req.headers()) else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let ok = {
        let store = state.inner.tokens.read().unwrap();
        store.validate(&secret)
    };
    if !ok {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Stash the validated secret for handlers that want to identify the caller.
    req.extensions_mut().insert(ValidatedSecret(secret));
    Ok(next.run(req).await)
}

#[derive(Clone, Debug)]
pub struct ValidatedSecret(pub String);
