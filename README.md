# AegisGraph

Real-time graph AML engine for instant payments. Self-hosted in your VPC; no PII leaves it.

Every payment is scored against a live, time-windowed graph of recent payments and shared identifiers
(devices, IPs, customers) **before it clears**. Every alert carries the exact payment path that triggered it,
so analysts and examiners see *why*, not just a number.

| | |
|---|---|
| Engine latency | **p99 9–40 µs** per payment, single thread, in-process |
| Throughput | **150k–400k payments/s** per core; **25k/s end to end** with Postgres persistence |
| Scale tested | 1M accounts, 77M payments, 5.7 GB RAM |
| Detection (independent IBM AML data) | ~1 in 5 alerts is laundering, **~130x** the base rate, at a 0.07% alert rate |

Full methodology, holdout runs and the honest caveats: [`bench/README.md`](bench/README.md).

## What it detects
| Rule | Pattern | Why it matters (UK APP fraud / PSR reimbursement) |
|---|---|---|
| `cycle` | Money returning to its origin via 2–4 hops | Layering / round-tripping |
| `mule_fan_in` | Many *new* senders → one (often young) account | Receiving mule collecting from victims |
| `pass_through` | Funds from strangers forwarded on within minutes | The core mule behaviour |
| `fan_out` | One account → many *new* payees | Scattering proceeds / account takeover |
| `velocity` | Bursts of payments to new payees | Draining a compromised account |
| `shared_identifier` | One device/IP/person behind many accounts | Mule herders |

**Counterparty memory** keeps each account's known payees and payers beyond the edge window, so payroll,
rent and salary spend don't alert. **Hub tagging** and **bounded search** keep latency flat when a
merchant or payroll provider touches everything.

## Quickstart
Full stack (needs Docker):
```bash
cd deploy
cp .env.example .env        # set AEGIS_API_KEY and POSTGRES_PASSWORD
docker compose up -d --build
```
Console: http://localhost:3000 (log in with `AEGIS_API_KEY`). Engine API: http://127.0.0.1:8080.

Score a payment:
```bash
curl -s -X POST http://127.0.0.1:8080/v1/score -H "Authorization: Bearer $AEGIS_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"tx_id":"t1","ts_ms":1700000000000,"amount_minor":100000,"currency":"GBP","debtor":"alice","creditor":"bob","scheme":"fps"}'
```

Without Docker, try the console against demo data: `cd console && npm ci && VITE_MOCK=1 npx vite`.

## Architecture
- `crates/aegis-core`: in-memory graph, detectors, noisy-OR scoring. Zero IO.
- `crates/aegis-server`: axum HTTP API, Postgres persistence, hash-chained audit log, restart rebuild ([API](docs/api.md)).
- `crates/aegis-bench`: benchmarks (synthetic, IBM AML) and the partner replay tool.
- `console/`: React + Cytoscape investigator UI: alert queue, path-highlighted subgraph, cases, dispositions.
- `deploy/`: Dockerfiles, Compose stack, Helm chart.
- `ee/`: enterprise modules (SSO, SAR generation, HA). Reserved.

Details: [`docs/architecture.md`](docs/architecture.md). Design-partner runbook: [`docs/pilot.md`](docs/pilot.md).

## Development
```bash
cargo test --workspace      # server tests start a throwaway Postgres automatically
cd console && npm ci && npm run dev
```

## Status
Pilot-ready core. The ML path (temporal graph network via ONNX) has an extension point (`MlScorer`) but no
model yet. On independent data, deterministic rules catch about 40% of laundering attempts; closing that gap is the
next milestone.

## License
BSL 1.1 (converts to Apache 2.0 on 2029-10-01). See [`LICENSE`](LICENSE).
