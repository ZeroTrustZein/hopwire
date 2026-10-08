//! Static bearer-key auth for `/v1/*`.

use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};

use crate::{error::AppError, AppState};

pub async fn require_key(State(st): State<AppState>, req: Request, next: Next) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match token {
        Some(t) if ct_eq(t.as_bytes(), st.api_key.as_bytes()) => Ok(next.run(req).await),
        _ => Err(AppError::unauthorized()),
    }
}

/// Constant-time comparison (length is not secret).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
