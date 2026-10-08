# aegis-server

axum HTTP API over the `aegis-core` engine, persisted in Postgres. The REST contract is
[`docs/api.md`](../../docs/api.md) (frozen).

## Layout
| File | Role |
|---|---|
| `src/main.rs` | Thin entry point: tracing, env config, serve |
| `src/lib.rs` | `AppState::init` / `AppState::with_pool`, `app(state) -> Router` |
| `src/actor.rs` | Single-writer engine actor: one OS thread owns `Engine`, commands arrive over one mpsc queue |
| `src/db.rs` | Pool and restart rebuild (replays `ts_ms >= max_ts - window_ms` through `Engine::ingest`) |
| `src/audit.rs` | Hash-chained `audit_log` (append under `pg_advisory_xact_lock`) and verify |
| `src/auth.rs` | Bearer-key middleware for `/v1/*` |
| `src/error.rs` | `{"error": "..."}` error type and extractor wrappers |
| `src/routes/` | `score.rs`, `alerts.rs`, `cases.rs`, plus stats, audit and metrics in `mod.rs` |
| `src/kafka.rs` | Feature `kafka`: doc-only seam, no dependencies |
| `migrations/` | sqlx migrations, embedded in the binary and run on startup |

Queries are checked at runtime (`sqlx::query*`, no `query!` macros), so the build needs neither a database nor
a `.sqlx` cache (`SQLX_OFFLINE=true` works).

## Run locally
```sh
export DATABASE_URL=postgres://aegis:aegis@localhost:5432/aegis
export AEGIS_API_KEY=dev-key
export AEGIS_CORS_ORIGIN=http://localhost:5173   # optional, console dev server
cargo run -p aegis-server --release

curl -s localhost:8080/healthz
curl -s -H "Authorization: Bearer dev-key" -H 'content-type: application/json' localhost:8080/v1/score \
  -d '{"tx_id":"t1","ts_ms":1790000000000,"amount_minor":125000,"currency":"GBP","debtor":"a","creditor":"b"}'
```

## Environment
| Var | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | required | Postgres URL |
| `AEGIS_API_KEY` | required | Static bearer key for `/v1/*` |
| `AEGIS_CONFIG` | none | Engine `config.toml`; defaults are used when unset |
| `AEGIS_BIND` | `0.0.0.0:8080` | Listen address |
| `AEGIS_CORS_ORIGIN` | none | Single allowed CORS origin |
| `RUST_LOG` | `info,sqlx=warn` | tracing filter |

The server writes nothing to the filesystem at runtime.

## Behaviour notes
- `POST /v1/score`: a duplicate `tx_id` returns `409` and the engine never sees it.
- `POST /v1/transactions`: `tx_id`s that are already stored, or repeated within the batch, are skipped. Replays are
  idempotent, and `accepted` counts only new txns.
- Every txn writes a `txn.scored` audit row. Non-allow verdicts also write `alert.created`. Other actions are
  `alert.disposition`, `case.created`, `case.updated` and `case.note`. `canonical_json` is compact JSON with object
  keys sorted recursively.
- `PATCH /v1/cases/{id}`: if `assignee` is absent it stays unchanged; `null` or `""` unassigns it.
- Alert subgraph: finding-path edges come first (they are never dropped by the cap), then the engine neighbourhoods
  of debtor and creditor. Edges are deduped on `(kind, from, to, tx_id)` and capped at 300.
- One engine per database. The graph is rebuilt from `transactions` on every start.

## Tests
```sh
cargo test -p aegis-server
```
`tests/api.rs` drives the router in-process (`tower::ServiceExt::oneshot`) against a real Postgres, using a fresh
database per scenario:
- If `AEGIS_TEST_DATABASE_URL` is set (CI's `postgres:17` service), that server is used. The URL needs rights to
  `CREATE DATABASE`.
- Otherwise `postgresql_embedded` downloads PostgreSQL binaries once (cached in `~/.theseus`, so the first run
  needs network) and starts a throwaway server. This works on Windows, macOS and Linux without Docker.

Scenarios: auth/401 and error shape, score → cycle alert (plus 409 on duplicate, detail, subgraph), batch ingest,
alert filters and pagination, disposition, cases (escalation, PATCH, notes), audit verify plus tamper detection
(and concurrent appends), restart rebuild, and stats/metrics.
