//! `POST /v1/score` and `POST /v1/transactions`.

use std::collections::HashSet;

use hopwire_core::{Decision, Txn, Verdict};
use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::enum_str;
use crate::{
    audit::{self, Entry},
    error::{ApiResult, AppError, AppJson},
    AppState,
};

const MAX_BATCH: usize = 10_000;

#[derive(Serialize)]
pub struct ScoreResponse {
    #[serde(flatten)]
    verdict: Verdict,
    alert_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct BatchResponse {
    pub accepted: usize,
    pub alerts: usize,
}

pub async fn score(
    State(st): State<AppState>,
    AppJson(txn): AppJson<Txn>,
) -> ApiResult<ScoreResponse> {
    validate(&txn)?;
    let mut db = st.pool.begin().await?;
    // Insert first: the unique tx_id makes duplicates (even concurrent ones) a 409
    // before the engine ever sees them.
    if insert_txns(&mut db, std::slice::from_ref(&txn))
        .await?
        .is_empty()
    {
        return Err(AppError::conflict(format!(
            "duplicate tx_id: {}",
            txn.tx_id
        )));
    }
    let verdict = st.engine.process(txn.clone()).await?;
    let alert_ids = record(
        &mut db,
        std::slice::from_ref(&txn),
        std::slice::from_ref(&verdict),
    )
    .await?;
    db.commit().await?;
    observe(std::slice::from_ref(&verdict));
    Ok(Json(ScoreResponse {
        verdict,
        alert_id: alert_ids[0],
    }))
}

#[derive(Serialize)]
pub struct Iso20022Item {
    pub tx_id: String,
    pub decision: Decision,
    pub score: f64,
    pub alert_id: Option<Uuid>,
    pub pacs002_xml: String,
}

/// Synchronous ISO 20022 pacs.008 customer credit transfer ingestion.
/// Returns pacs.002 XML response when scoring a single transfer.
pub async fn iso20022(State(st): State<AppState>, body: String) -> Result<Response, AppError> {
    let pacs = hopwire_iso20022::parse_pacs008(&body)
        .map_err(|e| AppError::bad_request(format!("invalid pacs.008 XML: {e}")))?;
    if pacs.is_empty() {
        return Err(AppError::bad_request("pacs.008 contains no transactions"));
    }

    let mut items = Vec::with_capacity(pacs.len());
    let mut db = st.pool.begin().await?;

    for p in &pacs {
        let txn = p.to_txn();
        validate(&txn)?;
        if insert_txns(&mut db, std::slice::from_ref(&txn))
            .await?
            .is_empty()
        {
            return Err(AppError::conflict(format!(
                "duplicate tx_id: {}",
                txn.tx_id
            )));
        }
        let verdict = st.engine.process(txn.clone()).await?;
        let alert_ids = record(
            &mut db,
            std::slice::from_ref(&txn),
            std::slice::from_ref(&verdict),
        )
        .await?;
        observe(std::slice::from_ref(&verdict));

        let top_reason = verdict
            .findings
            .first()
            .map(|f| f.reason.as_str())
            .unwrap_or("Transaction evaluated compliant");
        let pacs002 = hopwire_iso20022::generate_pacs002(
            &p.msg_id,
            &txn.tx_id,
            p.uetr.as_deref(),
            verdict.decision,
            top_reason,
        );

        items.push(Iso20022Item {
            tx_id: txn.tx_id,
            decision: verdict.decision,
            score: verdict.score,
            alert_id: alert_ids[0],
            pacs002_xml: pacs002,
        });
    }

    db.commit().await?;

    if items.len() == 1 {
        let xml = items.remove(0).pacs002_xml;
        Ok(([(header::CONTENT_TYPE, "application/xml")], xml).into_response())
    } else {
        Ok(Json(items).into_response())
    }
}

/// Replay path: same engine path, in array order. Txns whose `tx_id` is already
/// stored (or repeated within the batch) are skipped, so replays are idempotent.
pub async fn batch(
    State(st): State<AppState>,
    AppJson(txns): AppJson<Vec<Txn>>,
) -> ApiResult<BatchResponse> {
    Ok(Json(ingest_batch(&st, txns).await?))
}

pub async fn ingest_batch(st: &AppState, txns: Vec<Txn>) -> Result<BatchResponse, AppError> {
    if txns.len() > MAX_BATCH {
        return Err(AppError::bad_request(format!(
            "batch exceeds {MAX_BATCH} transactions"
        )));
    }
    txns.iter().try_for_each(validate)?;
    let mut db = st.pool.begin().await?;
    let mut fresh: HashSet<String> = insert_txns(&mut db, &txns).await?.into_iter().collect();
    let txns: Vec<Txn> = txns
        .into_iter()
        .filter(|t| fresh.remove(&t.tx_id))
        .collect();
    let verdicts = if txns.is_empty() {
        vec![]
    } else {
        st.engine.process_batch(txns.clone()).await?
    };
    let alerts = record(&mut db, &txns, &verdicts)
        .await?
        .into_iter()
        .flatten()
        .count();
    db.commit().await?;
    observe(&verdicts);
    Ok(BatchResponse {
        accepted: txns.len(),
        alerts,
    })
}

fn validate(t: &Txn) -> Result<(), AppError> {
    super::require("tx_id", &t.tx_id)?;
    super::require("debtor", &t.debtor)?;
    super::require("creditor", &t.creditor)?;
    super::require("currency", &t.currency)?;
    if t.amount_minor <= 0 {
        return Err(AppError::bad_request(format!(
            "{}: amount_minor must be > 0",
            t.tx_id
        )));
    }
    Ok(())
}

/// Insert, skipping existing tx_ids. Returns the tx_ids actually inserted.
async fn insert_txns(db: &mut PgConnection, txns: &[Txn]) -> Result<Vec<String>, AppError> {
    let raw: Vec<Value> = txns.iter().map(|t| json!(t)).collect();
    Ok(sqlx::query_scalar(
        "INSERT INTO transactions (tx_id, ts_ms, debtor, creditor, raw)
         SELECT r->>'tx_id', (r->>'ts_ms')::bigint, r->>'debtor', r->>'creditor', r
         FROM UNNEST($1::jsonb[]) WITH ORDINALITY AS u(r, n) ORDER BY n
         ON CONFLICT (tx_id) DO NOTHING
         RETURNING tx_id",
    )
    .bind(raw)
    .fetch_all(db)
    .await?)
}

/// Create alerts for non-allow verdicts and append audit rows. Returns per-txn alert id.
async fn record(
    db: &mut PgConnection,
    txns: &[Txn],
    verdicts: &[Verdict],
) -> Result<Vec<Option<Uuid>>, AppError> {
    let mut ids = Vec::with_capacity(txns.len());
    let mut audit_rows = Vec::with_capacity(txns.len());
    let mut a = AlertCols::default();
    for (t, v) in txns.iter().zip(verdicts) {
        let mut rules: Vec<String> = Vec::new();
        for f in &v.findings {
            let r = enum_str(&f.rule);
            if !rules.contains(&r) {
                rules.push(r);
            }
        }
        let decision = enum_str(&v.decision);
        let summary =
            json!({ "tx_id": t.tx_id, "score": v.score, "decision": decision, "rules": rules });
        audit_rows.push(Entry::new("api", "txn.scored", &t.tx_id, summary.clone()));
        if v.decision == Decision::Allow {
            ids.push(None);
            continue;
        }
        let id = Uuid::new_v4();
        let mut payload = summary;
        payload["alert_id"] = json!(id);
        audit_rows.push(Entry::new("api", "alert.created", id, payload));
        a.id.push(id);
        a.tx_id.push(t.tx_id.clone());
        a.ts_ms.push(t.ts_ms);
        a.score.push(v.score);
        a.decision.push(decision);
        a.rules.push(json!(rules));
        a.debtor.push(t.debtor.clone());
        a.creditor.push(t.creditor.clone());
        a.amount.push(t.amount_minor);
        a.currency.push(t.currency.clone());
        a.findings.push(json!(v.findings));
        ids.push(Some(id));
    }
    if !a.id.is_empty() {
        sqlx::query(
            "INSERT INTO alerts (id, tx_id, ts_ms, score, decision, rules, debtor, creditor, amount_minor, currency, findings)
             SELECT id, tx_id, ts_ms, score, decision, ARRAY(SELECT jsonb_array_elements_text(rules)),
                    debtor, creditor, amount_minor, currency, findings
             FROM UNNEST($1::uuid[], $2::text[], $3::int8[], $4::float8[], $5::text[], $6::jsonb[],
                         $7::text[], $8::text[], $9::int8[], $10::text[], $11::jsonb[])
                  WITH ORDINALITY AS u(id, tx_id, ts_ms, score, decision, rules,
                                       debtor, creditor, amount_minor, currency, findings, n)
             ORDER BY n",
        )
        .bind(a.id)
        .bind(a.tx_id)
        .bind(a.ts_ms)
        .bind(a.score)
        .bind(a.decision)
        .bind(a.rules)
        .bind(a.debtor)
        .bind(a.creditor)
        .bind(a.amount)
        .bind(a.currency)
        .bind(a.findings)
        .execute(&mut *db)
        .await?;
    }
    audit::append(db, audit_rows).await?;
    Ok(ids)
}

/// Column vectors for one bulk `UNNEST` alert insert.
#[derive(Default)]
struct AlertCols {
    id: Vec<Uuid>,
    tx_id: Vec<String>,
    ts_ms: Vec<i64>,
    score: Vec<f64>,
    decision: Vec<String>,
    rules: Vec<Value>,
    debtor: Vec<String>,
    creditor: Vec<String>,
    amount: Vec<i64>,
    currency: Vec<String>,
    findings: Vec<Value>,
}

/// Prometheus metrics, recorded only after the DB commit succeeded.
fn observe(verdicts: &[Verdict]) {
    metrics::counter!("hopwire_txns_total").increment(verdicts.len() as u64);
    for v in verdicts {
        metrics::histogram!("hopwire_score_latency_us").record(v.latency_us as f64);
        if v.decision != Decision::Allow {
            metrics::counter!("hopwire_alerts_total", "decision" => enum_str(&v.decision))
                .increment(1);
        }
    }
}
