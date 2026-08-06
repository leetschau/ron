//! REST API for Pulses.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Local, NaiveDate, NaiveDateTime};
use serde::Deserialize;

use crate::db;
use crate::id::{new_id, Kind};
use crate::models::{Interval, Pulse};
use crate::server::error::{ApiError, ApiResult};
use crate::server::notes::{delete_yaml, persist_yaml};
use crate::server::AppState;

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub active_only: Option<bool>,
}

async fn list(
    State(state): State<AppState>,
    Query(p): Query<ListParams>,
) -> ApiResult<Json<Vec<Pulse>>> {
    let pulses = {
        let conn = state.db();
        db::list_pulses(&conn)?
    };
    let out = match p.active_only {
        Some(true) => {
            let now = Local::now().naive_local();
            pulses.into_iter().filter(|p| p.is_active_at(now)).collect()
        }
        _ => pulses,
    };
    Ok(Json(out))
}

async fn get(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Pulse>> {
    let conn = state.db();
    db::get_pulse(&conn, &id)?.ok_or(ApiError::NotFound).map(Json)
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub topic: String,
    pub interval: Interval,
}

async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(axum::http::StatusCode, Json<Pulse>)> {
    if body.topic.trim().is_empty() {
        return Err(ApiError::BadRequest("topic must not be empty".into()));
    }
    let now = Local::now().naive_local();
    let pulse = Pulse::new(new_id(Kind::Pulse), body.topic, body.interval, now);
    {
        let conn = state.db();
        db::upsert_pulse(&conn, &pulse)?;
    }
    persist_yaml(&state, crate::yaml::Item::Pulse(pulse.clone()))?;
    Ok((axum::http::StatusCode::CREATED, Json(pulse)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateBody {
    pub topic: Option<String>,
    pub interval: Option<Interval>,
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Pulse>> {
    let mut pulse = {
        let conn = state.db();
        db::get_pulse(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    if let Some(t) = body.topic {
        if t.trim().is_empty() {
            return Err(ApiError::BadRequest("topic must not be empty".into()));
        }
        pulse.topic = t;
    }
    if let Some(i) = body.interval {
        pulse.interval = i;
    }
    {
        let conn = state.db();
        db::upsert_pulse(&conn, &pulse)?;
    }
    persist_yaml(&state, crate::yaml::Item::Pulse(pulse.clone()))?;
    Ok(Json(pulse))
}

#[derive(Debug, Deserialize)]
pub struct SlotParams {
    /// Slot key, e.g. `"2026-08-06"` for daily. Defaults to the current slot.
    pub on: Option<String>,
}

async fn check(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(p): Query<SlotParams>,
) -> ApiResult<Json<Pulse>> {
    set_slot(state, id, p.on, true).await
}

async fn uncheck(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(p): Query<SlotParams>,
) -> ApiResult<Json<Pulse>> {
    set_slot(state, id, p.on, false).await
}

async fn set_slot(
    state: AppState,
    id: String,
    on: Option<String>,
    checked: bool,
) -> ApiResult<Json<Pulse>> {
    let mut pulse = {
        let conn = state.db();
        db::get_pulse(&conn, &id)?.ok_or(ApiError::NotFound)?
    };
    let slot = match on {
        Some(s) => s,
        None => pulse.interval.current_slot(Local::now().naive_local()),
    };
    // Sanity-check daily slots parse as a date; other intervals are strings.
    validate_slot(&pulse.interval, &slot)?;
    pulse.set_slot(slot, checked);
    {
        let conn = state.db();
        db::upsert_pulse(&conn, &pulse)?;
    }
    persist_yaml(&state, crate::yaml::Item::Pulse(pulse.clone()))?;
    Ok(Json(pulse))
}

fn validate_slot(interval: &Interval, slot: &str) -> ApiResult<()> {
    let ok = match interval {
        Interval::Daily => NaiveDate::parse_from_str(slot, "%Y-%m-%d").is_ok(),
        Interval::Monthly | Interval::Weekly => {
            // Permissive: just require non-empty.
            !slot.is_empty()
        }
        Interval::Yearly => slot.parse::<i32>().is_ok() && slot.len() == 4,
    };
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "slot {slot:?} doesn't match interval {interval:?}"
        )))
    }
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = {
        let conn = state.db();
        db::delete_pulse(&conn, &id)?
    };
    if !removed {
        return Err(ApiError::NotFound);
    }
    delete_yaml(&state, &id)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[allow(dead_code)]
fn _unused(_d: NaiveDateTime) {}

pub fn routes() -> axum::Router<AppState> {
    use axum::routing;
    axum::Router::new()
        .route("/api/pulses", routing::get(list).post(create))
        .route("/api/pulses/:id", routing::get(get).put(update).delete(delete))
        .route("/api/pulses/:id/check", routing::post(check).delete(uncheck))
}
