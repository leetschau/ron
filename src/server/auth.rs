//! Authentication & authorization middleware.
//!
//! Two independent gates:
//!
//! * `require_token` — gates `/api/*` JSON routes with a bearer token. The
//!   token-management routes (`/api/tokens`) are exempt from bearer auth
//!   (they mint the first token), but their mutating methods (POST/DELETE)
//!   are restricted to loopback peers so that a LAN-reachable server can't
//!   be asked to mint a token by anyone but the local operator.
//!
//! * `require_viewer` — gates the browser/HTML routes with a `viewer_secret`
//!   cookie when `server.json` sets one. Absent `viewer_secret`, the viewer
//!   stays open (the historical behaviour).

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use std::net::SocketAddr;

use crate::server::AppState;

/// Cookie name carrying the viewer passphrase after a successful unlock.
pub const VIEWER_COOKIE: &str = "ron_viewer";

/// Cookie lifetime: 30 days, in seconds.
const VIEWER_COOKIE_MAX_AGE: u64 = 30 * 24 * 60 * 60;

/// Extract a bearer token from the `Authorization: Bearer <token>` header.
pub fn extract_bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?;
    let s = value.to_str().ok()?;
    let token = s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer "))?;
    Some(token.trim().to_string())
}

pub async fn require_token(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    let method = req.method().clone();

    // Token management routes are exempt from bearer auth (they bootstrap the
    // first token). But mutating methods are restricted to loopback peers so
    // a LAN snooper can't mint or revoke tokens against an externally-bound
    // server. See docs/phone-access.md.
    if path == "/api/tokens" && method == axum::http::Method::POST {
        require_loopback(&req)?;
        return Ok(next.run(req).await);
    }
    if path.starts_with("/api/tokens/") && method == axum::http::Method::DELETE {
        require_loopback(&req)?;
        return Ok(next.run(req).await);
    }
    // Non-mutating token routes (GET /api/tokens) stay open: they expose only
    // ids/labels/created, never secrets.

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

/// Reject the request unless the TCP peer is a loopback address. ConnectInfo
/// is injected via `into_make_service_with_connect_info` in `app::run`.
fn require_loopback(req: &Request) -> Result<(), StatusCode> {
    let ci = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .ok_or(StatusCode::FORBIDDEN)?;
    if ci.0.ip().is_loopback() {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Viewer gate. Applied only to the HTML viewer routes. When `viewer_secret`
/// is `None`, every request passes through (open viewer). When `Some(s)`:
///
/// * `/login` (GET/POST) and `/static/*` are always reachable.
/// * A correct `?key=<s>` query bootstrap sets the cookie and redirects to the
///   same path with the query stripped (so the secret never lingers in the
///   URL bar/history).
/// * Otherwise the `ron_viewer` cookie must match; missing/wrong → 302 to
///   `/login`.
pub async fn require_viewer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let secret = match &state.inner.viewer_secret {
        None => return next.run(req).await,
        Some(s) => s.clone(),
    };

    let path = req.uri().path().to_string();
    // Login and static assets are always reachable.
    if path == "/login" || path.starts_with("/static/") {
        return next.run(req).await;
    }

    // ?key=<secret> bootstrap: set cookie and redirect to the bare path.
    if req.method() == axum::http::Method::GET {
        if let Some(key) = query_pair(req.uri().query(), "key") {
            if key == secret {
                return redirect_with_cookie(&path, &secret);
            }
        }
    }

    // Cookie check.
    if let Some(val) = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(cookie_value)
    {
        if val == secret {
            return next.run(req).await;
        }
    }

    // Not authenticated → bounce to login.
    Redirect::to("/login").into_response()
}

#[derive(Clone, Debug)]
pub struct ValidatedSecret(pub String);

/// Build a `Set-Cookie` header value for the viewer session.
pub fn viewer_set_cookie(secret: &str) -> String {
    format!(
        "{VIEWER_COOKIE}={v}; Path=/; Max-Age={max}; HttpOnly; SameSite=Strict",
        v = secret,
        max = VIEWER_COOKIE_MAX_AGE,
    )
}

/// Redirect (302) to `path`, attaching the viewer Set-Cookie so the session
/// persists on the next request.
fn redirect_with_cookie(path: &str, secret: &str) -> Response {
    let mut resp = Redirect::to(path).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        axum::http::HeaderValue::from_str(&viewer_set_cookie(secret))
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
    );
    resp
}

/// Return the value of `name` in a `k=v&k2=v2` query string, if present.
fn query_pair(query: Option<&str>, name: &str) -> Option<String> {
    let q = query?;
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == name {
            return Some(url_decode(v));
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Extract the value of the `ron_viewer` cookie from a `Cookie:` header value.
fn cookie_value(header: &str) -> Option<String> {
    for pair in header.split(';') {
        let pair = pair.trim();
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == VIEWER_COOKIE {
            return Some(v.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_viewer_cookie() {
        assert_eq!(
            cookie_value("ron_viewer=hush; other=x"),
            Some("hush".into())
        );
        assert_eq!(cookie_value("theme=dark"), None);
        assert_eq!(cookie_value(""), None);
    }

    #[test]
    fn query_pair_decodes_value() {
        assert_eq!(query_pair(Some("key=abc&x=y"), "key"), Some("abc".into()));
        assert_eq!(query_pair(Some("key=hi%20there"), "key"), Some("hi there".into()));
        assert_eq!(query_pair(Some("key=a+b"), "key"), Some("a b".into()));
        assert_eq!(query_pair(None, "key"), None);
        assert_eq!(query_pair(Some("other=1"), "key"), None);
    }

    #[test]
    fn set_cookie_has_security_attrs() {
        let c = viewer_set_cookie("s3cr3t");
        assert!(c.starts_with("ron_viewer=s3cr3t;"));
        assert!(c.contains("HttpOnly"));
        assert!(c.contains("SameSite=Strict"));
        assert!(c.contains("Max-Age=2592000"));
    }
}
