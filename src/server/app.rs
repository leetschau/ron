//! Router assembly + server bootstrap.

use std::net::SocketAddr;

use anyhow::Result;
use axum::middleware;
use axum::Router;
use tokio::net::TcpListener;

use crate::paths::{Paths, ServerConfig};
use crate::server::{admin, auth, metrics, notes, pulses, tokens, AppState};
use crate::viewer;

/// Build the full application router with the bearer-auth layer applied.
pub fn build(state: AppState) -> Router {
    let api = Router::new()
        .merge(notes::routes())
        .merge(pulses::routes())
        .merge(metrics::routes())
        .merge(admin::routes())
        .merge(tokens::routes())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ))
        .with_state(state.clone());

    let viewer_routes = viewer::routes()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_viewer,
        ))
        .with_state(state.clone());

    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .merge(api)
        .merge(viewer_routes)
        .with_state(state)
}

/// Run the server until cancelled. Binds to `cfg.listen` (all interfaces by
/// default). Peer-IP checks (loopback-only `/api/tokens`) rely on the
/// `ConnectInfo<SocketAddr>` injected here.
pub async fn run(paths: Paths, cfg: ServerConfig) -> Result<()> {
    let state = AppState::new(paths.clone(), cfg.viewer_secret.clone())?;
    state.load_tokens()?;
    let app = build(state);
    let addr: SocketAddr = cfg.listen.parse()?;
    eprintln!("ron listening on http://{addr}");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
