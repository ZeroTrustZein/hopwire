# AegisGraph REST API v1 (pilot contract)

FROZEN contract between `aegis-server`, `console/`, and `aegis-bench --target`.
JSON shapes of `Txn`, `Verdict`, `Finding`, `PathEdge`, `Subgraph`, `EngineStats`, `Rule`,
`Decision`, `NodeKind`, `EdgeKind` are exactly the serde output of `crates/aegis-core/src/types.rs`
(enums are lowercase / snake_case strings).

## Conventions
- Auth: `Authorization: Bearer <AEGIS_API_KEY>` on every `/v1/*` route. Missing/wrong → `401`.
  `/healthz` and `/metrics` are open.
- Errors: non-2xx with body `{"error": "<message>"}`.
- IDs: UUID v4 strings. Times: `created_at`/`updated_at` RFC 3339 UTC strings; txn times stay `ts_ms` (epoch ms).
- Pagination: `?limit=` (default 50, max 500) `&offset=` (default 0) → `{"items": [...], "total": <int>}`.

## Server env
| Var | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | required | Postgres URL |
| `AEGIS_API_KEY` | required | Static bearer key (pilot; OIDC is EE) |
| `AEGIS_CONFIG` | none | Path to engine `config.toml`; defaults used if unset |
| `AEGIS_BIND` | `0.0.0.0:8080` | Listen address |
| `AEGIS_CORS_ORIGIN` | none | Allowed origin for console dev server (e.g. `http://localhost:5173`) |

## Scoring
### `POST /v1/score`
Body: `Txn`. Synchronous pre-clearance decision. Persists the txn; if `decision != "allow"` an alert is created.
200 → `ScoreResponse` = `Verdict` fields flattened + `"alert_id": string | null`.

### `POST /v1/transactions`
Body: `Txn[]` (max 10 000). Processed in array order through the same engine path. Used for log replay.
200 → `{"accepted": <int>, "alerts": <int>}`.

## Alerts
`AlertSummary`:
```json
{ "id": "uuid", "tx_id": "t1", "created_at": "2026-10-08T12:00:00Z", "ts_ms": 0,
  "score": 0.91, "decision": "review", "rules": ["cycle","pass_through"],
  "status": "open", "debtor": "acc1", "creditor": "acc2",
  "amount_minor": 125000, "currency": "GBP", "case_id": null }
```
`status`: `open` | `escalated` | `closed`.

- `GET /v1/alerts?status=&rule=&min_score=&limit=&offset=` → paginated `AlertSummary`, newest first.
- `GET /v1/alerts/{id}` → `AlertDetail` = `AlertSummary` + `"findings": Finding[]`, `"txn": Txn`, `"disposition": Disposition | null`.
- `GET /v1/alerts/{id}/subgraph?hops=2` (hops 1..=3, default 2) → `Subgraph`: union of engine neighbourhoods
  of debtor and creditor, **plus** every edge/node in the alert's finding paths (even if since evicted).
  Max 300 edges.
- `POST /v1/alerts/{id}/disposition` body `{"label": "true_positive"|"false_positive", "analyst": "name", "note": "optional"}`
  → `Disposition` `{ "alert_id", "label", "analyst", "note", "created_at" }`. Sets alert `status = closed`.
  Dispositions are the future ML label source.

## Cases
`Case`: `{ "id", "title", "status": "open"|"in_review"|"closed", "assignee": string|null, "created_at", "updated_at", "alert_ids": [] }`
`Note`: `{ "id", "case_id", "author", "body", "created_at" }`

- `POST /v1/cases` body `{"title", "alert_ids": [], "assignee"?}` → `Case`. Linked alerts get `status = escalated`, `case_id` set.
- `GET /v1/cases?status=&limit=&offset=` → paginated `Case`.
- `GET /v1/cases/{id}` → `Case` + `"alerts": AlertSummary[]`, `"notes": Note[]`.
- `PATCH /v1/cases/{id}` body `{"status"?, "assignee"?}` → `Case`.
- `POST /v1/cases/{id}/notes` body `{"author", "body"}` → `Note`.

## Ops
- `GET /v1/stats` → `EngineStats`.
- `GET /v1/audit/verify` → `{"ok": bool, "entries": <int>, "broken_at": <int|null>}`. Recomputes the hash chain.
- `GET /healthz` → `200 ok`.
- `GET /metrics` → Prometheus text (`aegis_txns_total`, `aegis_alerts_total{decision}`, `aegis_score_latency_us` histogram).

## Audit log
Every state change (txn scored, alert created, disposition, case create/update, note) appends one row:
`seq` (bigserial), `at`, `actor`, `action`, `entity_id`, `payload jsonb`, `prev_hash`, `hash`
where `hash = hex(sha256(prev_hash || canonical_json(payload) || action || entity_id))`, genesis `prev_hash` = 64 zeros.
Rows are never updated or deleted.
