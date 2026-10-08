//! Cases + notes.

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{alerts::alert_cols, alerts::AlertSummary, one_of, opt, paging, require, Page};
use crate::{
    audit::{self, Entry},
    error::{ApiResult, AppError, AppJson, AppPath, AppQuery},
    AppState,
};

/// `Case` columns; the cases table is always aliased `c`.
macro_rules! case_cols {
    () => {
        "c.id, c.title, c.status, c.assignee, c.created_at, c.updated_at,          COALESCE((SELECT array_agg(a.id ORDER BY a.seq) FROM alerts a WHERE a.case_id = c.id), '{}') AS alert_ids"
    };
}
const STATUSES: &[&str] = &["open", "in_review", "closed"];

#[derive(Serialize, sqlx::FromRow)]
pub struct Case {
    id: Uuid,
    title: String,
    status: String,
    assignee: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    alert_ids: Vec<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Note {
    id: Uuid,
    case_id: Uuid,
    author: String,
    body: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct CaseDetail {
    #[serde(flatten)]
    case: Case,
    alerts: Vec<AlertSummary>,
    notes: Vec<Note>,
}

#[derive(Deserialize)]
pub struct CreateReq {
    title: String,
    #[serde(default)]
    alert_ids: Vec<Uuid>,
    #[serde(default)]
    assignee: Option<String>,
}

pub async fn create(State(st): State<AppState>, AppJson(req): AppJson<CreateReq>) -> ApiResult<Case> {
    require("title", &req.title)?;
    let mut ids = req.alert_ids.clone();
    ids.sort();
    ids.dedup();
    let id = Uuid::new_v4();
    let assignee = req.assignee.filter(|a| !a.is_empty());
    let mut db = st.pool.begin().await?;
    sqlx::query("INSERT INTO cases (id, title, assignee) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(&req.title)
        .bind(&assignee)
        .execute(&mut *db)
        .await?;
    let linked = sqlx::query("UPDATE alerts SET status = 'escalated', case_id = $1 WHERE id = ANY($2)")
        .bind(id)
        .bind(&ids)
        .execute(&mut *db)
        .await?
        .rows_affected();
    if linked != ids.len() as u64 {
        return Err(AppError::bad_request("one or more alert_ids do not exist"));
    }
    let payload = json!({ "title": req.title, "assignee": assignee, "alert_ids": ids });
    audit::append(&mut db, vec![Entry::new("api", "case.created", id, payload)]).await?;
    let case = fetch_case(&mut db, id).await?;
    db.commit().await?;
    Ok(Json(case))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default, deserialize_with = "opt")]
    status: Option<String>,
    #[serde(default, deserialize_with = "opt")]
    limit: Option<i64>,
    #[serde(default, deserialize_with = "opt")]
    offset: Option<i64>,
}

pub async fn list(State(st): State<AppState>, AppQuery(q): AppQuery<ListQuery>) -> ApiResult<Page<Case>> {
    one_of("status", &q.status, STATUSES)?;
    let (limit, offset) = paging(q.limit, q.offset)?;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM cases WHERE ($1::text IS NULL OR status = $1)")
        .bind(&q.status)
        .fetch_one(&st.pool)
        .await?;
    let items = sqlx::query_as(concat!(
        "SELECT ",
        case_cols!(),
        " FROM cases c WHERE ($1::text IS NULL OR c.status = $1)
         ORDER BY c.created_at DESC, c.id LIMIT $2 OFFSET $3"
    ))
    .bind(&q.status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(Page { items, total }))
}

pub async fn get(State(st): State<AppState>, AppPath(id): AppPath<Uuid>) -> ApiResult<CaseDetail> {
    let mut conn = st.pool.acquire().await?;
    let case = fetch_case(&mut conn, id).await?;
    let alerts = sqlx::query_as(concat!("SELECT ", alert_cols!(), " FROM alerts a WHERE a.case_id = $1 ORDER BY a.seq"))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    let notes = sqlx::query_as(
        "SELECT id, case_id, author, body, created_at FROM case_notes WHERE case_id = $1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(CaseDetail { case, alerts, notes }))
}

#[derive(Deserialize)]
pub struct PatchReq {
    #[serde(default)]
    status: Option<String>,
    /// Absent → unchanged; `null` or `""` → unassign (NULL).
    #[serde(default, deserialize_with = "present")]
    assignee: Option<Option<String>>,
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(|v| Some(v.filter(|s| !s.is_empty())))
}

pub async fn patch(
    State(st): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(req): AppJson<PatchReq>,
) -> ApiResult<Case> {
    one_of("status", &req.status, STATUSES)?;
    let mut db = st.pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE cases SET status = COALESCE($2, status),
                          assignee = CASE WHEN $3 THEN $4 ELSE assignee END,
                          updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&req.status)
    .bind(req.assignee.is_some())
    .bind(req.assignee.clone().flatten())
    .execute(&mut *db)
    .await?
    .rows_affected();
    if updated == 0 {
        return Err(case_not_found(id));
    }
    let mut payload = json!({});
    if let Some(s) = &req.status {
        payload["status"] = json!(s);
    }
    if let Some(a) = &req.assignee {
        payload["assignee"] = json!(a);
    }
    audit::append(&mut db, vec![Entry::new("api", "case.updated", id, payload)]).await?;
    let case = fetch_case(&mut db, id).await?;
    db.commit().await?;
    Ok(Json(case))
}

#[derive(Deserialize)]
pub struct NoteReq {
    author: String,
    body: String,
}

pub async fn add_note(
    State(st): State<AppState>,
    AppPath(case_id): AppPath<Uuid>,
    AppJson(req): AppJson<NoteReq>,
) -> ApiResult<Note> {
    require("author", &req.author)?;
    require("body", &req.body)?;
    let mut db = st.pool.begin().await?;
    sqlx::query("SELECT 1 FROM cases WHERE id = $1")
        .bind(case_id)
        .fetch_optional(&mut *db)
        .await?
        .ok_or_else(|| case_not_found(case_id))?;
    let note: Note = sqlx::query_as(
        "INSERT INTO case_notes (id, case_id, author, body) VALUES ($1, $2, $3, $4)
         RETURNING id, case_id, author, body, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(case_id)
    .bind(&req.author)
    .bind(&req.body)
    .fetch_one(&mut *db)
    .await?;
    let payload = json!({ "case_id": case_id, "author": note.author, "body": note.body });
    audit::append(&mut db, vec![Entry::new(&req.author, "case.note", note.id, payload)]).await?;
    db.commit().await?;
    Ok(Json(note))
}

async fn fetch_case(conn: &mut PgConnection, id: Uuid) -> Result<Case, AppError> {
    sqlx::query_as(concat!("SELECT ", case_cols!(), " FROM cases c WHERE c.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| case_not_found(id))
}

fn case_not_found(id: Uuid) -> AppError {
    AppError::not_found(format!("case {id} not found"))
}
