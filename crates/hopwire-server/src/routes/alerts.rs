//! Alert queue, detail, subgraph, disposition.

use std::collections::HashSet;

use axum::{extract::State, Json};
use chrono::{DateTime, Utc};
use hopwire_core::{Finding, NodeKind, PathEdge, SubNode, Subgraph, Txn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as Jsonb;
use uuid::Uuid;

use super::{one_of, opt, paging, require, Page};
use crate::{
    audit::{self, Entry},
    error::{ApiResult, AppError, AppJson, AppPath, AppQuery},
    AppState,
};

const MAX_SUBGRAPH_EDGES: usize = 300;

/// `AlertSummary` columns; the alerts table is always aliased `a`.
macro_rules! alert_cols {
    () => {
        "a.id, a.tx_id, a.created_at, a.ts_ms, a.score, a.decision, a.rules, a.status, \
         a.debtor, a.creditor, a.amount_minor, a.currency, a.case_id"
    };
}
pub(super) use alert_cols;

#[derive(Serialize, sqlx::FromRow)]
pub struct AlertSummary {
    id: Uuid,
    tx_id: String,
    created_at: DateTime<Utc>,
    ts_ms: i64,
    score: f64,
    decision: String,
    rules: Vec<String>,
    status: String,
    debtor: String,
    creditor: String,
    amount_minor: i64,
    currency: String,
    case_id: Option<Uuid>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AlertDetail {
    #[serde(flatten)]
    #[sqlx(flatten)]
    summary: AlertSummary,
    findings: Jsonb<Value>,
    txn: Jsonb<Value>,
    #[sqlx(skip)]
    disposition: Option<Disposition>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Disposition {
    alert_id: Uuid,
    label: String,
    analyst: String,
    note: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default, deserialize_with = "opt")]
    status: Option<String>,
    #[serde(default, deserialize_with = "opt")]
    rule: Option<String>,
    #[serde(default, deserialize_with = "opt")]
    min_score: Option<f64>,
    #[serde(default, deserialize_with = "opt")]
    limit: Option<i64>,
    #[serde(default, deserialize_with = "opt")]
    offset: Option<i64>,
}

pub async fn list(
    State(st): State<AppState>,
    AppQuery(q): AppQuery<ListQuery>,
) -> ApiResult<Page<AlertSummary>> {
    one_of("status", &q.status, &["open", "escalated", "closed"])?;
    let (limit, offset) = paging(q.limit, q.offset)?;
    macro_rules! filter {
        () => {
            " FROM alerts a WHERE ($1::text IS NULL OR a.status = $1)
              AND ($2::text IS NULL OR $2 = ANY(a.rules))
              AND ($3::float8 IS NULL OR a.score >= $3)"
        };
    }
    let total: i64 = sqlx::query_scalar(concat!("SELECT count(*)", filter!()))
        .bind(&q.status)
        .bind(&q.rule)
        .bind(q.min_score)
        .fetch_one(&st.pool)
        .await?;
    let items = sqlx::query_as(concat!(
        "SELECT ",
        alert_cols!(),
        filter!(),
        " ORDER BY a.seq DESC LIMIT $4 OFFSET $5"
    ))
    .bind(&q.status)
    .bind(&q.rule)
    .bind(q.min_score)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(Page { items, total }))
}

pub async fn get(State(st): State<AppState>, AppPath(id): AppPath<Uuid>) -> ApiResult<AlertDetail> {
    let mut detail: AlertDetail = sqlx::query_as(concat!(
        "SELECT ",
        alert_cols!(),
        ", a.findings, t.raw AS txn FROM alerts a JOIN transactions t ON t.tx_id = a.tx_id WHERE a.id = $1"
    ))
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(|| alert_not_found(id))?;
    detail.disposition = sqlx::query_as(
        "SELECT alert_id, label, analyst, note, created_at FROM dispositions WHERE alert_id = $1",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    Ok(Json(detail))
}

#[derive(Deserialize)]
pub struct SubgraphQuery {
    #[serde(default, deserialize_with = "opt")]
    hops: Option<u8>,
}

/// Engine neighbourhoods of debtor + creditor, plus every finding-path edge/node
/// (even if evicted since). Path edges take priority under the 300-edge cap.
pub async fn subgraph(
    State(st): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(q): AppQuery<SubgraphQuery>,
) -> ApiResult<Subgraph> {
    let hops = q.hops.unwrap_or(2);
    if !(1..=3).contains(&hops) {
        return Err(AppError::bad_request("hops must be between 1 and 3"));
    }
    let (debtor, creditor, Jsonb(findings)): (String, String, Jsonb<Vec<Finding>>) =
        sqlx::query_as("SELECT debtor, creditor, findings FROM alerts WHERE id = $1")
            .bind(id)
            .fetch_optional(&st.pool)
            .await?
            .ok_or_else(|| alert_not_found(id))?;
    let engine_graphs = st
        .engine
        .subgraphs(vec![debtor, creditor], hops, MAX_SUBGRAPH_EDGES)
        .await?;

    let mut seen = HashSet::new();
    let mut edges: Vec<PathEdge> = Vec::new();
    let path_edges = findings.into_iter().flat_map(|f| f.path);
    let engine_edges = engine_graphs.iter().flat_map(|g| g.edges.iter().cloned());
    for e in path_edges.chain(engine_edges) {
        if edges.len() == MAX_SUBGRAPH_EDGES {
            break;
        }
        if seen.insert((e.kind, e.from.clone(), e.to.clone(), e.tx_id.clone())) {
            edges.push(e);
        }
    }

    // Nodes: engine nodes first (they carry the hub flag), then any path endpoint the
    // engine no longer knows; only nodes touched by a kept edge (or a lone root).
    let endpoints: HashSet<&str> = edges
        .iter()
        .flat_map(|e| [e.from.as_str(), e.to.as_str()])
        .collect();
    let mut node_ids = HashSet::new();
    let mut nodes: Vec<SubNode> = Vec::new();
    for n in engine_graphs.iter().flat_map(|g| &g.nodes) {
        if (endpoints.contains(n.id.as_str()) || edges.is_empty()) && node_ids.insert(n.id.clone())
        {
            nodes.push(n.clone());
        }
    }
    for e in &edges {
        for id in [&e.from, &e.to] {
            if node_ids.insert(id.clone()) {
                nodes.push(SubNode {
                    id: id.clone(),
                    kind: node_kind(id),
                    hub: false,
                });
            }
        }
    }
    Ok(Json(Subgraph { nodes, edges }))
}

/// External ID prefix convention from `hopwire_core::NodeKind` docs.
fn node_kind(id: &str) -> NodeKind {
    if id.starts_with("device:") {
        NodeKind::Device
    } else if id.starts_with("ip:") {
        NodeKind::Ip
    } else if id.starts_with("cust:") {
        NodeKind::Customer
    } else {
        NodeKind::Account
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    TruePositive,
    FalsePositive,
}

#[derive(Deserialize)]
pub struct DispositionReq {
    label: Label,
    analyst: String,
    #[serde(default)]
    note: Option<String>,
}

pub async fn disposition(
    State(st): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(req): AppJson<DispositionReq>,
) -> ApiResult<Disposition> {
    require("analyst", &req.analyst)?;
    let mut db = st.pool.begin().await?;
    sqlx::query("SELECT 1 FROM alerts WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *db)
        .await?
        .ok_or_else(|| alert_not_found(id))?;
    let d: Disposition = sqlx::query_as(
        "INSERT INTO dispositions (alert_id, label, analyst, note) VALUES ($1, $2, $3, $4)
         ON CONFLICT (alert_id) DO NOTHING
         RETURNING alert_id, label, analyst, note, created_at",
    )
    .bind(id)
    .bind(super::enum_str(&req.label))
    .bind(&req.analyst)
    .bind(&req.note)
    .fetch_optional(&mut *db)
    .await?
    .ok_or_else(|| AppError::conflict(format!("alert {id} already has a disposition")))?;
    sqlx::query("UPDATE alerts SET status = 'closed' WHERE id = $1")
        .bind(id)
        .execute(&mut *db)
        .await?;
    let payload = json!({ "alert_id": id, "label": d.label, "analyst": d.analyst, "note": d.note });
    audit::append(
        &mut db,
        vec![Entry::new(&req.analyst, "alert.disposition", id, payload)],
    )
    .await?;
    db.commit().await?;
    Ok(Json(d))
}

#[derive(Serialize)]
pub struct SarExportResponse {
    pub alert_id: Uuid,
    pub narrative: hopwire_ee::sar::GeneratedSarNarrative,
    pub fincen_xml: String,
    pub goaml_xml: String,
}

pub async fn generate_sar(
    State(st): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<SarExportResponse> {
    let row: Option<(Jsonb<Txn>, Jsonb<Vec<hopwire_core::Finding>>, f64)> = sqlx::query_as(
        "SELECT t.raw, a.findings, a.score FROM alerts a JOIN transactions t ON a.tx_id = t.tx_id WHERE a.id = $1"
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;

    let (Jsonb(txn), Jsonb(findings), score) = row.ok_or_else(|| alert_not_found(id))?;

    let narrative =
        hopwire_ee::sar::synthesize_narrative(&txn, &findings, score, "Hopwire Pilot Bank");

    let fincen_data = hopwire_ee::sar::fincen::FinCenSarData {
        filing_institution: "Hopwire Pilot Bank".into(),
        bsar_id: id.to_string(),
        narrative: narrative.full_markdown.clone(),
        suspicious_activity_from: chrono::DateTime::from_timestamp_millis(txn.ts_ms)
            .map(|dt| dt.format("%Y%m%d").to_string())
            .unwrap_or_else(|| "20261008".into()),
        suspicious_activity_to: chrono::DateTime::from_timestamp_millis(txn.ts_ms)
            .map(|dt| dt.format("%Y%m%d").to_string())
            .unwrap_or_else(|| "20261008".into()),
        total_amount_minor: txn.amount_minor,
        currency: txn.currency.clone(),
        subjects: vec![
            hopwire_ee::sar::fincen::FinCenSubject {
                name: txn.debtor.clone(),
                account_id: txn.debtor.clone(),
                role: "Debtor".into(),
                address: None,
                tax_id: txn.debtor_customer_hash.clone(),
            },
            hopwire_ee::sar::fincen::FinCenSubject {
                name: txn.creditor.clone(),
                account_id: txn.creditor.clone(),
                role: "Creditor".into(),
                address: None,
                tax_id: txn.creditor_customer_hash.clone(),
            },
        ],
        typologies: vec!["MoneyLaunderingLayering".into(), "FraudMuleNetwork".into()],
    };
    let fincen_xml = hopwire_ee::sar::fincen::generate_fincen_xml(&fincen_data);

    let goaml_data = hopwire_ee::sar::uk_fiu::UkFiuSarData {
        report_id: id.to_string(),
        reporting_entity: "Hopwire Pilot Bank".into(),
        reason_for_suspicion: narrative.full_markdown.clone(),
        action_taken: "Account settlement frozen; customer due diligence refreshed".into(),
        parties: vec![
            hopwire_ee::sar::uk_fiu::GoAmlParty {
                client_number: txn.debtor.clone(),
                name: txn.debtor.clone(),
                role: "Originator".into(),
                account_number: txn.debtor.clone(),
                institution_name: "Originating PSP".into(),
            },
            hopwire_ee::sar::uk_fiu::GoAmlParty {
                client_number: txn.creditor.clone(),
                name: txn.creditor.clone(),
                role: "Beneficiary".into(),
                account_number: txn.creditor.clone(),
                institution_name: "Hopwire Pilot Bank".into(),
            },
        ],
        transactions: vec![hopwire_ee::sar::uk_fiu::GoAmlTransaction {
            tx_id: txn.tx_id.clone(),
            date: chrono::DateTime::from_timestamp_millis(txn.ts_ms)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default(),
            amount_minor: txn.amount_minor,
            currency: txn.currency.clone(),
            from_account: txn.debtor.clone(),
            to_account: txn.creditor.clone(),
            description: format!("Suspicious payment flagged by Hopwire: {}", narrative.title),
        }],
    };
    let goaml_xml = hopwire_ee::sar::uk_fiu::generate_goaml_xml(&goaml_data);

    Ok(Json(SarExportResponse {
        alert_id: id,
        narrative,
        fincen_xml,
        goaml_xml,
    }))
}

fn alert_not_found(id: Uuid) -> AppError {
    AppError::not_found(format!("alert {id} not found"))
}
