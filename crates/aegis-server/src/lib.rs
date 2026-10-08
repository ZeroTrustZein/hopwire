//! AegisGraph HTTP server: axum API over a single-writer `aegis_core::Engine`
//! actor, persisted in Postgres. Contract: `docs/api.md`.

pub mod actor;
pub mod audit;
mod auth;
pub mod config;
pub mod db;
pub mod error;
#[cfg(feature = "kafka")]
pub mod kafka;
pub mod metrics;
mod routes;

use std::sync::Arc;

use anyhow::Result;
use axum::{http::HeaderValue, middleware, routing::get, Router};
use sqlx::PgPool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub use config::ServerConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub engine: actor::EngineHandle,
    pub api_key: Arc<str>,
    pub cors_origin: Option<String>,
    pub metrics: metrics_exporter_prometheus::PrometheusHandle,
}

impl AppState {
    /// Connect, migrate, spawn the engine actor and rebuild its window from Postgres.
    pub async fn init(cfg: &ServerConfig) -> Result<Self> {
        let pool = db::connect(&cfg.database_url).await?;
        Self::with_pool(pool, cfg).await
    }

    /// Like [`AppState::init`] but over an existing pool (migrations still run).
    pub async fn with_pool(pool: PgPool, cfg: &ServerConfig) -> Result<Self> {
        sqlx::migrate!("./migrations").run(&pool).await?;
        let engine = actor::EngineHandle::spawn(cfg.engine.clone());
        let replayed = db::rebuild(&pool, &engine, cfg.engine.window_ms).await?;
        tracing::info!(replayed, "engine state rebuilt from Postgres");
        Ok(Self {
            pool,
            engine,
            api_key: cfg.api_key.as_str().into(),
            cors_origin: cfg.cors_origin.clone(),
            metrics: metrics::handle(),
        })
    }
}

pub fn app(state: AppState) -> Router {
    let v1 = routes::v1().route_layer(middleware::from_fn_with_state(state.clone(), auth::require_key));
    let mut router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(routes::metrics))
        .merge(v1)
        .fallback(routes::not_found)
        .method_not_allowed_fallback(routes::method_not_allowed)
        .with_state(state.clone())
        .layer(TraceLayer::new_for_http());
    if let Some(origin) = state.cors_origin.as_deref().and_then(|o| HeaderValue::from_str(o).ok()) {
        router = router.layer(
            CorsLayer::new()
                .allow_origin(origin)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        );
    }
    router
}
