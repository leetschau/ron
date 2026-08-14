//! REST API for Metrics.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Local, NaiveDateTime};
use serde::Serialize;

use crate::db;
use crate::id::{new_id, Kind};
use crate::models::{Metric, MetricPoint};
use crate::server::error::{ApiError, ApiResult};
use crate::server::notes::{delete_yaml, persist_yaml};
use crate::server::AppState;

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<Metric>>> {
    let metrics = {
        let conn = state.db();
        db::list_metrics(&conn)?
    };
    Ok(Json(metrics))
}

async fn get(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Metric>> {
    let conn = state.db();
    db::get_metric(&conn, &id)?.ok_or(ApiError::NotFound).map(Json)
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateBody {
    pub topic: String,
}

async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(axum::http::StatusCode, Json<Metric>)> {
    let metric = create_metric_inner(&state, body).await?;
    Ok((axum::http::StatusCode::CREATED, Json(metric)))
}

/// Shared create logic (used by the JSON API and the viewer's create form).
pub async fn create_metric_inner(state: &AppState, body: CreateBody) -> ApiResult<Metric> {
    if body.topic.trim().is_empty() {
        return Err(ApiError::BadRequest("topic must not be empty".into()));
    }
    let now = Local::now().naive_local();
    let metric = Metric::new(new_id(Kind::Metric), body.topic, now);
    {
        let conn = state.db();
        db::upsert_metric(&conn, &metric)?;
    }
    persist_yaml(state, crate::yaml::Item::Metric(metric.clone()))?;
    Ok(metric)
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateBody {
    pub topic: Option<String>,
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Metric>> {
    let metric = update_metric_inner(&state, &id, body).await?;
    Ok(Json(metric))
}

/// Shared update logic (used by the JSON API and the viewer's edit form).
pub async fn update_metric_inner(
    state: &AppState,
    id: &str,
    body: UpdateBody,
) -> ApiResult<Metric> {
    let mut metric = {
        let conn = state.db();
        db::get_metric(&conn, id)?.ok_or(ApiError::NotFound)?
    };
    if let Some(t) = body.topic {
        if t.trim().is_empty() {
            return Err(ApiError::BadRequest("topic must not be empty".into()));
        }
        metric.topic = t;
    }
    {
        let conn = state.db();
        db::upsert_metric(&conn, &metric)?;
    }
    persist_yaml(state, crate::yaml::Item::Metric(metric.clone()))?;
    Ok(metric)
}

#[derive(Debug, serde::Deserialize)]
pub struct AppendBody {
    pub value: f64,
    pub ts: Option<NaiveDateTime>,
}

async fn append_point(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AppendBody>,
) -> ApiResult<Json<Metric>> {
    let metric = append_point_inner(&state, &id, body).await?;
    Ok(Json(metric))
}

/// Shared append logic (used by the JSON API and the viewer's log form).
pub async fn append_point_inner(
    state: &AppState,
    id: &str,
    body: AppendBody,
) -> ApiResult<Metric> {
    let mut metric = {
        let conn = state.db();
        db::get_metric(&conn, id)?.ok_or(ApiError::NotFound)?
    };
    let ts = body.ts.unwrap_or_else(|| Local::now().naive_local());
    metric.append(ts, body.value);
    {
        let conn = state.db();
        db::upsert_metric(&conn, &metric)?;
    }
    persist_yaml(state, crate::yaml::Item::Metric(metric.clone()))?;
    Ok(metric)
}

#[derive(Debug, serde::Deserialize)]
pub struct StatsQuery {
    pub from: Option<NaiveDateTime>,
    pub to: Option<NaiveDateTime>,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub topic: String,
    pub count: usize,
    pub mean: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    pub points: Vec<MetricPoint>,
}

async fn stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Json<StatsResponse>> {
    let metric = {
        let conn = state.db();
        db::get_metric(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let stats = metric
        .stats(q.from, q.to)
        .ok_or(ApiError::NotFound)?;
    let points: Vec<MetricPoint> = metric
        .sorted_points()
        .into_iter()
        .filter(|p| q.from.map_or(true, |f| p.ts >= f))
        .filter(|p| q.to.map_or(true, |t| p.ts <= t))
        .cloned()
        .collect();
    Ok(Json(StatsResponse {
        topic: metric.topic,
        count: stats.count,
        mean: stats.mean,
        median: stats.median,
        min: stats.min,
        max: stats.max,
        points,
    }))
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = delete_metric_inner(&state, &id).await?;
    if !removed {
        return Err(ApiError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Shared delete logic (used by the JSON API and the viewer's delete form).
pub async fn delete_metric_inner(state: &AppState, id: &str) -> ApiResult<bool> {
    let removed = {
        let conn = state.db();
        db::delete_metric(&conn, id)?
    };
    if removed {
        delete_yaml(state, id)?;
    }
    Ok(removed)
}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/metrics", routing::get(list).post(create))
        .route("/api/metrics/:id", routing::get(get).put(update).delete(delete))
        .route("/api/metrics/:id/points", routing::post(append_point))
        .route("/api/metrics/:id/stats", routing::get(stats))
}
