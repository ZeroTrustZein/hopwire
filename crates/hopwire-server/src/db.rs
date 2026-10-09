//! Postgres pool, embedded migrations, and restart rebuild.

use hopwire_core::Txn;
use anyhow::{Context, Result};
use futures_util::TryStreamExt;
use sqlx::{postgres::PgPoolOptions, types::Json, PgPool};

use crate::actor::EngineHandle;

/// Connect only; migrations run in `AppState::with_pool`.
pub async fn connect(url: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(16)
        .connect(url)
        .await
        .context("connecting to Postgres")
}

/// Replay the last `history_ms` of transactions (relative to the newest stored
/// `ts_ms`) through `Engine::ingest`, in `ts_ms` then insertion order. Returns rows replayed.
pub async fn rebuild(pool: &PgPool, engine: &EngineHandle, history_ms: i64) -> Result<u64> {
    let Some(max_ts) = sqlx::query_scalar::<_, Option<i64>>("SELECT max(ts_ms) FROM transactions")
        .fetch_one(pool)
        .await?
    else {
        return Ok(0);
    };
    let mut rows = sqlx::query_scalar::<_, Json<Txn>>(
        "SELECT raw FROM transactions WHERE ts_ms >= $1 ORDER BY ts_ms, seq",
    )
    .bind(max_ts.saturating_sub(history_ms))
    .fetch(pool);

    const CHUNK: usize = 1_000;
    let (mut n, mut buf) = (0u64, Vec::with_capacity(CHUNK));
    while let Some(Json(t)) = rows.try_next().await? {
        buf.push(t);
        if buf.len() == CHUNK {
            n += CHUNK as u64;
            engine
                .ingest(std::mem::replace(&mut buf, Vec::with_capacity(CHUNK)))
                .await?;
        }
    }
    n += buf.len() as u64;
    engine.ingest(buf).await?;
    Ok(n)
}
