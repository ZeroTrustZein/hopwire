//! End-to-end API tests against a real Postgres.
//!
//! Postgres source: `AEGIS_TEST_DATABASE_URL` if set (CI service container),
//! otherwise a throwaway server via `postgresql_embedded` (binaries downloaded
//! once and cached under `~/.theseus`). Each scenario gets its own fresh database.
//!
//! One `#[tokio::test]` drives every scenario so the embedded server is started
//! once and reliably stopped (its `Drop`) even when a scenario panics.

use std::str::FromStr;

use aegis_server::{app, AppState, ServerConfig};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, ConnectOptions, PgPool};
use tower::ServiceExt;

const KEY: &str = "test-key";
const H: i64 = 3_600_000;
const T0: i64 = 1_790_000_000_000;

#[tokio::test(flavor = "multi_thread")]
async fn api_suite() {
    let (_pg, base_url) = postgres().await;
    let base = PgConnectOptions::from_str(&base_url).unwrap();
    macro_rules! run {
        ($($name:ident),* $(,)?) => {$(
            let pool = fresh_db(&base, stringify!($name)).await;
            $name(pool).await;
            eprintln!("scenario {} ok", stringify!($name));
        )*};
    }
    run!(
        auth_and_errors,
        score_cycle_creates_alert,
        batch_ingest,
        alert_filters_and_pagination,
        disposition_closes_alert,
        cases_and_notes,
        audit_verify_and_tamper,
        restart_rebuild,
        ops_endpoints,
    );
}

// ---------------------------------------------------------------- scenarios

async fn auth_and_errors(pool: PgPool) {
    let a = TestApp::new(pool).await;
    for auth in [None, Some("Bearer nope"), Some(KEY)] {
        let mut req = Request::get("/v1/alerts");
        if let Some(v) = auth {
            req = req.header("authorization", v);
        }
        let (s, body) = a.send(req.body(Body::empty()).unwrap()).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert_eq!(body, json!({"error": "unauthorized"}));
    }
    assert_eq!(a.raw("/healthz").await, (StatusCode::OK, "ok".into()));
    let (s, body) = a
        .get("/v1/alerts/00000000-0000-4000-8000-000000000000")
        .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(body["error"].is_string());
    let (s, body) = a.get("/v1/alerts/not-a-uuid").await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(body["error"].is_string());
    let (s, body) = a.post("/v1/score", json!({"tx_id": "x"})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["error"].is_string());
}

async fn score_cycle_creates_alert(pool: PgPool) {
    let a = TestApp::new(pool).await;
    let [l1, l2, l3] = cycle("c");
    for leg in [&l1, &l2] {
        let (s, v) = a.post("/v1/score", leg.clone()).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["decision"], "allow");
        assert_eq!(v["alert_id"], Value::Null);
    }
    let (s, v) = a.post("/v1/score", l3.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_ne!(v["decision"], "allow", "{v}");
    assert_eq!(v["tx_id"], "c3");
    assert!(v["latency_us"].is_u64() && v["score"].is_f64());
    let cyc = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "cycle")
        .expect("cycle finding");
    assert_eq!(cyc["path"].as_array().unwrap().len(), 3);
    let id = v["alert_id"].as_str().expect("alert_id").to_string();

    // Duplicate tx_id → 409, nothing new persisted.
    let (s, body) = a.post("/v1/score", l3.clone()).await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert!(body["error"].as_str().unwrap().contains("c3"));

    let (s, d) = a.get(&format!("/v1/alerts/{id}")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["id"], id.as_str());
    assert_eq!(d["tx_id"], "c3");
    assert_eq!(d["status"], "open");
    assert_eq!(d["debtor"], "c-acc3");
    assert_eq!(d["creditor"], "c-acc1");
    assert_eq!(d["amount_minor"], 90_000);
    assert_eq!(d["currency"], "GBP");
    assert_eq!(d["case_id"], Value::Null);
    assert_eq!(d["disposition"], Value::Null);
    assert!(d["rules"].as_array().unwrap().contains(&json!("cycle")));
    assert_eq!(d["findings"], v["findings"]);
    assert_eq!(d["txn"], l3);
    assert!(chrono::DateTime::parse_from_rfc3339(d["created_at"].as_str().unwrap()).is_ok());

    // Subgraph contains every path edge with identical values, and its endpoints.
    let (s, g) = a.get(&format!("/v1/alerts/{id}/subgraph")).await;
    assert_eq!(s, StatusCode::OK);
    let edges = g["edges"].as_array().unwrap();
    for pe in cyc["path"].as_array().unwrap() {
        assert!(edges.contains(pe), "path edge {pe} missing from subgraph");
    }
    let ids: Vec<&str> = g["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    for n in ["c-acc1", "c-acc2", "c-acc3"] {
        assert!(ids.contains(&n));
    }
    let mut keys: Vec<_> = edges
        .iter()
        .map(|e| {
            (
                e["kind"].clone(),
                e["from"].clone(),
                e["to"].clone(),
                e["tx_id"].clone(),
            )
        })
        .map(|k| format!("{k:?}"))
        .collect();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n, "subgraph edges deduped");
    assert_eq!(
        a.get(&format!("/v1/alerts/{id}/subgraph?hops=1")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        a.get(&format!("/v1/alerts/{id}/subgraph?hops=4")).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        a.get(&format!("/v1/alerts/{id}/subgraph?hops=0")).await.0,
        StatusCode::BAD_REQUEST
    );
}

async fn batch_ingest(pool: PgPool) {
    let a = TestApp::new(pool.clone()).await;
    let mut txns: Vec<Value> = ["x", "y"].iter().flat_map(|p| cycle(p)).collect();
    txns.push(txn("plain", "p-a", "p-b", T0, 5_000));
    let (s, r) = a.post("/v1/transactions", json!(txns)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r, json!({"accepted": 7, "alerts": 2}));
    // Replay is idempotent: known tx_ids are skipped.
    let (s, r) = a.post("/v1/transactions", json!(txns)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(r, json!({"accepted": 0, "alerts": 0}));
    assert_eq!(count(&pool, "transactions").await, 7);
    let (_, list) = a.get("/v1/alerts").await;
    assert_eq!(list["total"], 2);
    let (_, stats) = a.get("/v1/stats").await;
    assert_eq!(stats["processed"], 7);
    let too_many: Vec<Value> = (0..10_001)
        .map(|i| txn(&format!("m{i}"), "m-a", "m-b", T0, 1))
        .collect();
    assert_eq!(
        a.post("/v1/transactions", json!(too_many)).await.0,
        StatusCode::BAD_REQUEST
    );
    let (s, _) = a
        .post("/v1/transactions", json!([txn("bad", "a", "b", T0, 0)]))
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

async fn alert_filters_and_pagination(pool: PgPool) {
    let a = TestApp::new(pool).await;
    let txns: Vec<Value> = ["f1", "f2", "f3"].iter().flat_map(|p| cycle(p)).collect();
    a.post("/v1/transactions", json!(txns)).await;

    let (_, all) = a.get("/v1/alerts").await;
    assert_eq!(all["total"], 3);
    let items = all["items"].as_array().unwrap();
    assert_eq!(items[0]["tx_id"], "f33", "newest first");
    assert_eq!(items[2]["tx_id"], "f13");
    assert!(
        items[0].get("findings").is_none(),
        "summary has no findings"
    );

    let (_, page) = a.get("/v1/alerts?limit=1&offset=1").await;
    assert_eq!(page["total"], 3);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["tx_id"], "f23");
    assert_eq!(a.get("/v1/alerts?offset=5").await.1["items"], json!([]));

    assert_eq!(a.get("/v1/alerts?rule=cycle").await.1["total"], 3);
    assert_eq!(a.get("/v1/alerts?rule=fan_out").await.1["total"], 0);
    assert_eq!(a.get("/v1/alerts?min_score=0.0").await.1["total"], 3);
    assert_eq!(a.get("/v1/alerts?min_score=1.01").await.1["total"], 0);
    assert_eq!(
        a.get("/v1/alerts?status=&rule=&min_score=").await.1["total"],
        3
    );

    let id = items[0]["id"].as_str().unwrap();
    a.post(
        &format!("/v1/alerts/{id}/disposition"),
        json!({"label": "false_positive", "analyst": "ann"}),
    )
    .await;
    assert_eq!(a.get("/v1/alerts?status=open").await.1["total"], 2);
    let (_, closed) = a.get("/v1/alerts?status=closed").await;
    assert_eq!(closed["total"], 1);
    assert_eq!(closed["items"][0]["id"], id);
    assert_eq!(
        a.get("/v1/alerts?status=bogus").await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        a.get("/v1/alerts?limit=-1").await.0,
        StatusCode::BAD_REQUEST
    );
}

async fn disposition_closes_alert(pool: PgPool) {
    let a = TestApp::new(pool).await;
    let id = make_alert(&a, "d").await;
    let url = format!("/v1/alerts/{id}/disposition");
    assert_eq!(
        a.post(&url, json!({"label": "maybe", "analyst": "ann"}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        a.post(&url, json!({"label": "true_positive", "analyst": ""}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (s, d) = a
        .post(
            &url,
            json!({"label": "true_positive", "analyst": "ann", "note": "mule ring"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{d}");
    assert_eq!(d["alert_id"], id.as_str());
    assert_eq!(d["label"], "true_positive");
    assert_eq!(d["analyst"], "ann");
    assert_eq!(d["note"], "mule ring");
    assert!(d["created_at"].is_string());
    let (_, detail) = a.get(&format!("/v1/alerts/{id}")).await;
    assert_eq!(detail["status"], "closed");
    assert_eq!(detail["disposition"], d);
    assert_eq!(
        a.post(&url, json!({"label": "false_positive", "analyst": "bob"}))
            .await
            .0,
        StatusCode::CONFLICT
    );
    let missing = "/v1/alerts/00000000-0000-4000-8000-000000000000/disposition";
    assert_eq!(
        a.post(missing, json!({"label": "true_positive", "analyst": "a"}))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

async fn cases_and_notes(pool: PgPool) {
    let a = TestApp::new(pool).await;
    let (al1, al2, al3) = (
        make_alert(&a, "k1").await,
        make_alert(&a, "k2").await,
        make_alert(&a, "k3").await,
    );

    let (s, c) = a
        .post(
            "/v1/cases",
            json!({"title": "Ring A", "alert_ids": [al1, al2], "assignee": "ann"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{c}");
    assert_eq!(c["title"], "Ring A");
    assert_eq!(c["status"], "open");
    assert_eq!(c["assignee"], "ann");
    assert_eq!(c["alert_ids"], json!([al1, al2]));
    assert!(c["created_at"].is_string() && c["updated_at"].is_string());
    let cid = c["id"].as_str().unwrap().to_string();
    for al in [&al1, &al2] {
        let (_, d) = a.get(&format!("/v1/alerts/{al}")).await;
        assert_eq!(d["status"], "escalated");
        assert_eq!(d["case_id"], cid.as_str());
    }
    assert_eq!(
        a.get(&format!("/v1/alerts/{al3}")).await.1["status"],
        "open"
    );
    assert_eq!(a.get("/v1/alerts?status=escalated").await.1["total"], 2);

    let (s, c2) = a.post("/v1/cases", json!({"title": "Empty"})).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(c2["alert_ids"], json!([]));
    assert_eq!(c2["assignee"], Value::Null);
    let bad = json!({"title": "x", "alert_ids": ["00000000-0000-4000-8000-000000000000"]});
    assert_eq!(a.post("/v1/cases", bad).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        a.post("/v1/cases", json!({"title": ""})).await.0,
        StatusCode::BAD_REQUEST
    );

    let (s, n) = a
        .post(
            &format!("/v1/cases/{cid}/notes"),
            json!({"author": "ann", "body": "called bank"}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{n}");
    assert_eq!(n["case_id"], cid.as_str());
    assert_eq!(n["author"], "ann");
    assert_eq!(n["body"], "called bank");
    assert!(n["id"].is_string() && n["created_at"].is_string());
    let missing = "/v1/cases/00000000-0000-4000-8000-000000000000/notes";
    assert_eq!(
        a.post(missing, json!({"author": "a", "body": "b"})).await.0,
        StatusCode::NOT_FOUND
    );

    let (s, p) = a
        .patch(&format!("/v1/cases/{cid}"), json!({"status": "in_review"}))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(p["status"], "in_review");
    assert_eq!(p["assignee"], "ann", "absent assignee unchanged");
    let (_, p) = a
        .patch(&format!("/v1/cases/{cid}"), json!({"assignee": "bob"}))
        .await;
    assert_eq!(
        (p["status"].as_str(), p["assignee"].as_str()),
        (Some("in_review"), Some("bob"))
    );
    let (_, p) = a
        .patch(&format!("/v1/cases/{cid}"), json!({"assignee": ""}))
        .await;
    assert_eq!(p["assignee"], Value::Null, "empty string unassigns");
    let (_, p) = a
        .patch(&format!("/v1/cases/{cid}"), json!({"assignee": "cy"}))
        .await;
    assert_eq!(p["assignee"], "cy");
    let (_, p) = a
        .patch(&format!("/v1/cases/{cid}"), json!({"assignee": null}))
        .await;
    assert_eq!(p["assignee"], Value::Null, "null unassigns");
    assert_eq!(
        a.patch(&format!("/v1/cases/{cid}"), json!({"status": "done"}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let missing = "/v1/cases/00000000-0000-4000-8000-000000000000";
    assert_eq!(
        a.patch(missing, json!({"status": "closed"})).await.0,
        StatusCode::NOT_FOUND
    );

    let (s, d) = a.get(&format!("/v1/cases/{cid}")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(d["id"], cid.as_str());
    assert_eq!(d["alert_ids"], json!([al1, al2]));
    let alert_ids: Vec<&Value> = d["alerts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| &x["id"])
        .collect();
    assert_eq!(alert_ids, [&json!(al1), &json!(al2)]);
    assert_eq!(d["notes"], json!([n]));

    let (_, list) = a.get("/v1/cases").await;
    assert_eq!(list["total"], 2);
    assert_eq!(list["items"][0]["title"], "Empty", "newest first");
    assert_eq!(a.get("/v1/cases?status=in_review").await.1["total"], 1);
    assert_eq!(
        a.get("/v1/cases?status=open&limit=1").await.1["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        a.get("/v1/cases?status=nope").await.0,
        StatusCode::BAD_REQUEST
    );
}

async fn audit_verify_and_tamper(pool: PgPool) {
    let a = TestApp::new(pool.clone()).await;
    let (_, v) = a.get("/v1/audit/verify").await;
    assert_eq!(v, json!({"ok": true, "entries": 0, "broken_at": null}));

    let al = make_alert(&a, "u").await; // 3 txn.scored + 1 alert.created
    a.post(
        &format!("/v1/alerts/{al}/disposition"),
        json!({"label": "true_positive", "analyst": "ann"}),
    )
    .await;
    let (_, c) = a
        .post("/v1/cases", json!({"title": "t", "alert_ids": [al]}))
        .await;
    let cid = c["id"].as_str().unwrap();
    a.patch(&format!("/v1/cases/{cid}"), json!({"status": "closed"}))
        .await;
    a.post(
        &format!("/v1/cases/{cid}/notes"),
        json!({"author": "ann", "body": "b"}),
    )
    .await;

    let (_, v) = a.get("/v1/audit/verify").await;
    assert_eq!(v, json!({"ok": true, "entries": 8, "broken_at": null}));
    let actions: Vec<String> = sqlx::query_scalar("SELECT action FROM audit_log ORDER BY seq")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        actions,
        [
            "txn.scored",
            "txn.scored",
            "txn.scored",
            "alert.created",
            "alert.disposition",
            "case.created",
            "case.updated",
            "case.note"
        ]
    );

    // Concurrent appends keep the chain linear.
    let url = format!("/v1/cases/{cid}/notes");
    let reqs = (0..8).map(|i| a.post(&url, json!({"author": "x", "body": format!("n{i}")})));
    for (s, _) in futures_util::future::join_all(reqs).await {
        assert_eq!(s, StatusCode::OK);
    }
    assert_eq!(
        a.get("/v1/audit/verify").await.1,
        json!({"ok": true, "entries": 16, "broken_at": null})
    );

    let seq: i64 = sqlx::query_scalar("SELECT seq FROM audit_log ORDER BY seq OFFSET 4 LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE audit_log SET payload = jsonb_set(payload, '{label}', '\"false_positive\"') WHERE seq = $1")
        .bind(seq)
        .execute(&pool)
        .await
        .unwrap();
    let (_, v) = a.get("/v1/audit/verify").await;
    assert_eq!(v, json!({"ok": false, "entries": 16, "broken_at": seq}));
}

async fn restart_rebuild(pool: PgPool) {
    let [l1, l2, l3] = cycle("r");
    {
        let a = TestApp::new(pool.clone()).await;
        for leg in [&l1, &l2] {
            assert_eq!(
                a.post("/v1/score", leg.clone()).await.1["decision"],
                "allow"
            );
        }
        // History older than history_ms (35d) must not be replayed.
        a.post(
            "/v1/score",
            txn("old", "o-a", "o-b", T0 - 40 * 24 * H, 1_000),
        )
        .await;
    } // engine actor dropped here = "process restart"

    let a = TestApp::new(pool).await;
    let (_, stats) = a.get("/v1/stats").await;
    assert_eq!(
        stats["processed"], 2,
        "rebuild replays only history_ms: {stats}"
    );
    assert_eq!(
        stats["watermark_ms"],
        T0 + 2 * H,
        "watermark = last replayed leg"
    );
    let (s, v) = a.post("/v1/score", l3).await;
    assert_eq!(s, StatusCode::OK);
    assert_ne!(v["decision"], "allow", "cycle detected after restart: {v}");
    assert!(v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["rule"] == "cycle"));
    assert!(v["alert_id"].is_string());
}

async fn ops_endpoints(pool: PgPool) {
    let a = TestApp::new(pool).await;
    make_alert(&a, "m").await;
    let (s, st) = a.get("/v1/stats").await;
    assert_eq!(s, StatusCode::OK);
    for k in ["nodes", "edges", "hubs", "processed", "watermark_ms"] {
        assert!(st[k].is_number(), "{k} in {st}");
    }
    let (s, text) = a.raw("/metrics").await;
    assert_eq!(s, StatusCode::OK);
    for m in [
        "aegis_txns_total",
        "aegis_alerts_total{decision=",
        "aegis_score_latency_us_bucket",
    ] {
        assert!(text.contains(m), "{m} missing from /metrics:\n{text}");
    }
}

// ---------------------------------------------------------------- helpers

fn txn(id: &str, from: &str, to: &str, ts: i64, amount: i64) -> Value {
    json!({
        "tx_id": id, "ts_ms": ts, "amount_minor": amount, "currency": "GBP",
        "debtor": from, "creditor": to, "scheme": "fps", "device_id": null, "ip": null,
        "debtor_customer_hash": null, "creditor_customer_hash": null, "creditor_account_opened_at_ms": null
    })
}

/// 3-hop cycle acc1 → acc2 → acc3 → acc1, legs 2h apart (outside the 60-min pass-through window).
fn cycle(p: &str) -> [Value; 3] {
    let acc = |i: u8| format!("{p}-acc{i}");
    [
        txn(&format!("{p}1"), &acc(1), &acc(2), T0, 100_000),
        txn(&format!("{p}2"), &acc(2), &acc(3), T0 + 2 * H, 95_000),
        txn(&format!("{p}3"), &acc(3), &acc(1), T0 + 4 * H, 90_000),
    ]
}

async fn make_alert(a: &TestApp, p: &str) -> String {
    let mut last = Value::Null;
    for leg in cycle(p) {
        last = a.post("/v1/score", leg).await.1;
    }
    last["alert_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no alert: {last}"))
        .to_string()
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

struct TestApp {
    router: Router,
}

impl TestApp {
    async fn new(pool: PgPool) -> Self {
        let cfg = ServerConfig {
            database_url: String::new(),
            api_key: KEY.into(),
            bind: String::new(),
            cors_origin: None,
            engine: aegis_core::Config::default(),
        };
        Self {
            router: app(AppState::with_pool(pool, &cfg).await.unwrap()),
        }
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn raw(&self, uri: &str) -> (StatusCode, String) {
        let res = self
            .router
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.send(
            Request::get(uri)
                .header("authorization", format!("Bearer {KEY}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    async fn post(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.json("POST", uri, body).await
    }

    async fn patch(&self, uri: &str, body: Value) -> (StatusCode, Value) {
        self.json("PATCH", uri, body).await
    }

    async fn json(&self, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {KEY}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        self.send(req).await
    }
}

/// Returns (guard keeping embedded Postgres alive, admin URL).
async fn postgres() -> (Option<postgresql_embedded::PostgreSQL>, String) {
    if let Ok(url) = std::env::var("AEGIS_TEST_DATABASE_URL") {
        return (None, url);
    }
    let mut pg = postgresql_embedded::PostgreSQL::default();
    pg.setup().await.expect("embedded postgres setup");
    pg.start().await.expect("embedded postgres start");
    let url = pg.settings().url("postgres");
    (Some(pg), url)
}

async fn fresh_db(base: &PgConnectOptions, scenario: &str) -> PgPool {
    let name = format!("aegis_test_{scenario}_{}", uuid::Uuid::new_v4().simple());
    let mut admin = base.connect().await.expect("connect admin db");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut admin)
        .await
        .unwrap();
    PgPool::connect_with(base.clone().database(&name))
        .await
        .unwrap()
}
