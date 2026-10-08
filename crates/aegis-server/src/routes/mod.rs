//! HTTP handlers. Shapes follow `docs/api.md` exactly.

mod alerts;
mod cases;
mod score;

use std::{fmt::Display, str::FromStr};

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::{
    audit,
    error::{ApiResult, AppError},
    AppState,
};

/// All `/v1/*` routes (auth is layered on by `crate::app`).
pub fn v1() -> Router<AppState> {
    Router::new()
        .route("/v1/score", post(score::score))
        .route(
            "/v1/transactions",
            // 10 000 txns comfortably exceed axum's 2 MB default.
            post(score::batch).layer(DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route("/v1/alerts", get(alerts::list))
        .route("/v1/alerts/{id}", get(alerts::get))
        .route("/v1/alerts/{id}/subgraph", get(alerts::subgraph))
        .route("/v1/alerts/{id}/disposition", post(alerts::disposition))
        .route("/v1/cases", get(cases::list).post(cases::create))
        .route("/v1/cases/{id}", get(cases::get).patch(cases::patch))
        .route("/v1/cases/{id}/notes", post(cases::add_note))
        .route("/v1/stats", get(stats))
        .route("/v1/audit/verify", get(audit_verify))
}

pub async fn metrics(State(st): State<AppState>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], st.metrics.render())
}

pub async fn not_found() -> AppError {
    AppError::not_found("not found")
}

pub async fn method_not_allowed() -> AppError {
    AppError::new(StatusCode::METHOD_NOT_ALLOWED, "method not allowed")
}

async fn stats(State(st): State<AppState>) -> ApiResult<aegis_core::EngineStats> {
    Ok(Json(st.engine.stats().await?))
}

async fn audit_verify(State(st): State<AppState>) -> ApiResult<audit::Verify> {
    Ok(Json(audit::verify(&st.pool).await?))
}

#[derive(Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// `(limit, offset)` with the contract defaults: limit 50, max 500; offset 0.
fn paging(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), AppError> {
    let (l, o) = (limit.unwrap_or(50), offset.unwrap_or(0));
    if l < 0 || o < 0 {
        return Err(AppError::bad_request("limit and offset must be >= 0"));
    }
    Ok((l.min(500), o))
}

/// Query-string field: absent or empty (`?status=`) → `None`, else `FromStr`.
fn opt<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    match Option::<String>::deserialize(d)? {
        Some(s) if !s.is_empty() => s.parse().map(Some).map_err(de::Error::custom),
        _ => Ok(None),
    }
}

/// Serde string form of a unit enum (`Decision::Review` → `"review"`).
fn enum_str<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn require(field: &str, v: &str) -> Result<(), AppError> {
    if v.trim().is_empty() {
        return Err(AppError::bad_request(format!("{field} must not be empty")));
    }
    Ok(())
}

fn one_of(field: &str, v: &Option<String>, allowed: &[&str]) -> Result<(), AppError> {
    match v {
        Some(s) if !allowed.contains(&s.as_str()) => {
            Err(AppError::bad_request(format!("{field} must be one of {}", allowed.join(", "))))
        }
        _ => Ok(()),
    }
}
