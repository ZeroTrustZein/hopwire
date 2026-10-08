# AegisGraph pilot runbook (design partners)

Audience: the partner's engineering lead (sections 1–4, 9) and fraud-ops / MLRO lead (sections 5–8, 10).
Goal: in 14 days, replay your own payment history through AegisGraph inside your VPC and measure
whether it catches the mule and APP-fraud cases you already know about, at an alert volume your team can work.

## 1. Prerequisites
- One Linux VM (or laptop for a dry run) inside your network: 4 vCPU, 16 GB RAM, 50 GB disk is plenty for a pilot.
- Docker Engine 24+ with the Compose plugin (`docker compose version`).
- Git access to this repository (or the two images, `aegisgraph/engine` and `aegisgraph/console`, pushed to your registry).
- A historical export of payments: ideally 3–6 months, including the transactions behind cases you have already
  confirmed (mule accounts, APP-fraud victims' outbound payments). Keep the list of confirmed-bad `tx_id`s/accounts aside;
  it is the answer key for section 10.
- Optional (for the replay tool outside Docker): Rust toolchain, `cargo run -p aegis-bench --release -- ...`.

No outbound internet access is needed at runtime. Images can be built elsewhere and pushed to your internal registry.

## 2. Ten-minute local bring-up
```bash
git clone <repo> aegisgraph && cd aegisgraph/deploy
cp .env.example .env
# Edit .env: set AEGIS_API_KEY and POSTGRES_PASSWORD (e.g. `openssl rand -hex 32` for each)
docker compose up -d --build
docker compose ps                         # postgres healthy, engine + console up
curl -s http://127.0.0.1:8080/healthz     # -> ok
```
Open http://localhost:3000 and log in with the `AEGIS_API_KEY` value.

Smoke test: three accounts paying each other in a loop should produce a `cycle` finding on the third payment.
```bash
KEY=<your AEGIS_API_KEY>
t=$(($(date +%s) * 1000))
for p in "A B 1" "B C 2" "C A 3"; do set -- $p
  curl -s -X POST http://127.0.0.1:8080/v1/score \
    -H "Authorization: Bearer $KEY" -H 'Content-Type: application/json' \
    -d "{\"tx_id\":\"smoke-$3\",\"ts_ms\":$((t + $3 * 60000)),\"amount_minor\":250000,\"currency\":\"GBP\",\"debtor\":\"smoke-$1\",\"creditor\":\"smoke-$2\",\"scheme\":\"fps\"}"
  echo
done
```
The last response is expected to show `"decision"` of `review` or `block`, a `cycle` finding whose `path` is the
three payments, and a non-null `alert_id`.
The alert appears in the console queue.

Stop with `docker compose down`. `docker compose down -v` also **deletes the Postgres volume (all transactions, alerts,
dispositions and the audit log)** — only do that to reset a dry run.

## 3. Mapping your export to the `Txn` schema
The engine accepts one JSON object per payment. Every ID is an opaque string: the engine only compares IDs for equality,
so hashed values work exactly as well as raw ones.

| Field | Type | Required | Meaning / example |
|---|---|---|---|
| `tx_id` | string | yes | Your unique payment reference. `"FPS-20260901-000123"` |
| `ts_ms` | integer | yes | Execution time, Unix epoch **milliseconds** UTC. `1788220800000` |
| `amount_minor` | integer | yes | Amount in minor units (pence/cents), > 0. £1,250.00 → `125000` |
| `currency` | string | yes | ISO 4217. `"GBP"` |
| `debtor` | string | yes | Paying account ID (see formats below) |
| `creditor` | string | yes | Receiving account ID |
| `scheme` | string | no | `fps`, `chaps`, `bacs`, `card`, `internal`, `other` (default `other`) |
| `device_id` | string | no | Device fingerprint used to initiate the payment |
| `ip` | string | no | IP address the payment was initiated from |
| `debtor_customer_hash` | string | no | Hashed identity of the paying customer (links accounts of one person) |
| `creditor_customer_hash` | string | no | Hashed identity of the receiving customer, if they are your customer |
| `creditor_account_opened_at_ms` | integer | no | When the receiving account was opened (epoch ms). Young accounts raise mule-fan-in scores |

Account ID formats — pick one per field and apply it consistently across the whole export:
- UK FPS/BACS/CHAPS: sort code + account number, digits only: `"040004:12345678"`.
- IBAN: uppercase, no spaces: `"GB33BUKB20201555555555"`.
- Card: a token or PAN hash, never a raw PAN.

Optional fields matter: `device_id`, `ip` and the customer hashes power the shared-identifier detector, and
`creditor_account_opened_at_ms` boosts mule detection. Provide what you have.

### PII: hash before export
No raw personal data needs to enter AegisGraph. Hash account numbers, IBANs, device IDs, IPs and customer IDs with
**HMAC-SHA256 under a key that only you hold** (a "salt" that is secret and constant for the pilot). Without the key the
hashes cannot be reversed by dictionary attack; with it, your team can re-identify any account the console flags by
hashing your own records the same way. Do not hash `tx_id` if you need to look payments up in your core system (or keep
a mapping). Example mapper, CSV in, JSON Lines out:

```python
# map_export.py: python map_export.py export.csv > txns.jsonl   (key in env AEGIS_PILOT_KEY)
import csv, hmac, hashlib, json, os, sys
from datetime import datetime, timezone

KEY = os.environ["AEGIS_PILOT_KEY"].encode()
def h(v):
    return hmac.new(KEY, v.strip().upper().replace(" ", "").encode(), hashlib.sha256).hexdigest() if v else None

rows = []
for r in csv.DictReader(open(sys.argv[1], newline="")):
    ts = datetime.fromisoformat(r["executed_at"]).replace(tzinfo=timezone.utc)   # adjust column names
    rows.append({
        "tx_id": r["payment_id"],
        "ts_ms": int(ts.timestamp() * 1000),
        "amount_minor": round(float(r["amount"]) * 100),   # better: parse as Decimal
        "currency": r["currency"],
        "debtor": h(f'{r["debtor_sort_code"]}:{r["debtor_account"]}'),
        "creditor": h(f'{r["creditor_sort_code"]}:{r["creditor_account"]}'),
        "scheme": "fps",
        "device_id": h(r.get("device_id")),
        "ip": h(r.get("ip")),
        "debtor_customer_hash": h(r.get("customer_id")),
    })
rows.sort(key=lambda t: t["ts_ms"])            # the engine must see history in time order
for t in rows:
    print(json.dumps({k: v for k, v in t.items() if v is not None}))
```
Apply the same normalisation (here: strip spaces, uppercase) before hashing every time, or the same account will hash to two IDs.

## 4. Replaying history
Replay into a **fresh** stack dedicated to the pilot. The engine's sliding window is driven by the latest timestamp it
has seen, so the file must be sorted by `ts_ms` ascending, and mixing a historical replay with live traffic on one
instance will evict edges early.

Include at least `history_ms` (35 days) of history *before* the period you want to evaluate. The first weeks teach the
engine your customers' regular counterparties; alerts raised in that warm-up are mostly first-time payroll and bills
and should be excluded from the false-positive count. On our synthetic benchmark, precision is 9 % with no history
and 65–88 % after a 35-day warm-up, at the same recall (see `bench/README.md`).

The engine image ships the replay tool, so nothing else needs installing:
```bash
cd deploy
KEY=$(grep '^AEGIS_API_KEY=' .env | cut -d= -f2-)
docker compose run --rm -v "$PWD/../pilot-data:/data:ro" --entrypoint /usr/local/bin/aegis-bench \
  engine replay --target http://engine:8080 --api-key "$KEY" --input /data/txns.jsonl
```
(Or from a machine with Rust: `cargo run -p aegis-bench --release -- replay --target http://127.0.0.1:8080 --api-key ... --input txns.jsonl`.)

The tool posts batches to `POST /v1/transactions`, which scores every payment through the same code path as live
`POST /v1/score` and persists alerts. Run `aegis-bench replay --help` for batching options and the exact input format.
Without the tool, any script that POSTs JSON arrays of up to 10 000 `Txn` objects (in order) to `/v1/transactions`
works the same way.

Re-running with a different config: edit `config.toml` (section 5), then reset (`docker compose down -v && docker compose up -d`)
and replay again, so each run starts from an empty graph.

## 5. Tuning `config.toml` (for the fraud-ops lead)
Copy the defaults, point Compose at the copy, restart the engine:
```bash
cp ../config.example.toml config.toml
echo 'AEGIS_CONFIG_FILE=./config.toml' >> .env
docker compose up -d engine        # first time (picks up the new mount)
docker compose restart engine      # after later edits to config.toml
```
On restart the engine rebuilds its in-memory graph from the transactions stored in Postgres, using the new settings.

How a score is made: each detector that fires produces a finding with a confidence between 0 and 1. The finding is
multiplied by that detector's **weight** (how much you trust the rule), and the weighted findings are combined so that
several weak signals add up but the total never exceeds 1. Two findings of 0.5 give 0.75, not 1.0. A single detector
can never push a score above its own weight.

**Decision bands** (`[bands]`): score ≥ `review` (default 0.5) creates an alert for analysts; score ≥ `block` (default
0.85) is the "stop the payment" recommendation. Raise `review` to cut alert volume; lower it to catch more.

**Global**
- `window_ms` (default 72 h): how long payment edges stay in the graph. Ring patterns spread over longer than this are not seen.
- `history_ms` (default 35 days): how much history is replayed on restart. Edges older than `window_ms` are dropped
  again, but the engine keeps remembering *who has paid whom*. That memory is what stops a monthly payroll or
  a regular rent payment from looking like a sudden scatter to new payees. Set it to cover your longest regular pay cycle.
- `hub_degree` (default 500): an account or device with this many counterparties in the window is treated as a hub
  (payroll provider, merchant, shared office IP). Hubs still score their own payments but are not walked through when
  looking for rings, which keeps latency flat and avoids "everyone is connected via Amazon" false positives.

**Counterparty memory.** Every account keeps a bounded record of payees and payers it has dealt with before (up to
16 384 each; the least-recently-paid half is dropped when full, so regular counterparties survive). Fan-out,
velocity, fan-in and pass-through only count *new* counterparties, and a 2-hop "A pays B, B pays A back" between
established parties is scored at half strength. That is why the first weeks after go-live alert more than steady
state: backfill at least one pay cycle of history before judging alert volume (section 4).

| Detector | Plain English | Knobs (defaults) |
|---|---|---|
| `cycle` | Money comes back to where it started via 2–4 accounts (round-tripping, layering). Loops that return less than half the value are scored down. | `max_hops` 4, `max_span_ms` 72 h (whole loop must close within), `budget` 2000 (search effort cap; leave alone), `weight` 0.9 |
| `fan_in` | One account suddenly receives from many *new* senders: the classic APP mule collecting from victims. Card-scheme inflows (merchants) are ignored; scored higher if the account is new. | `min_senders` 6 within `window_ms` 24 h, established accounts need `min_new_senders` 6, `young_account_ms` 30 days, `weight` 0.58 |
| `pass_through` | Money from *new* payers lands and is forwarded on almost immediately: the core mule behaviour. Salary from a known employer spent on payday does not count. | `min_ratio` 0.8 (≥ 80 % of inflow sent out) `within_ms` 60 min, ignored below `min_inflow_minor` 10 000 (£100), `weight` 0.7 |
| `fan_out` | One account pays many *new* payees: scattering stolen funds, or a compromised account being emptied. Boosted when payees are newly opened accounts. | `min_payees` 10 within `window_ms` 24 h, `young_payee_ms` 30 days, `weight` 0.6 |
| `velocity` | Unusually many or large payments to new payees in a short period. | `max_count` 15 or `max_sum_minor` 50 000 000 (£500 000) within `window_ms` 1 h, `weight` 0.7 |
| `shared_id` | One device, IP or person is behind several accounts: mule herders running accounts from one phone. | `min_accounts` 4, `weight` 0.85 |

Every detector has `enabled = true|false`. Tuning loop for the pilot: replay → disposition a sample of alerts → look at
which rules drive false positives → raise that rule's threshold or lower its weight → replay again. Change one thing at a time.

## 6. Reading alerts in the console
- **Alert queue**: newest first; filter by rule, minimum score, and status (`open`, `escalated`, `closed`).
- **Alert detail**: the score and decision, one plain-English reason per finding, the triggering transaction, and a
  graph of the surrounding accounts with the path that caused the alert highlighted (e.g. the loop for `cycle`, the
  senders for `fan_in`). The path is the explanation: every highlighted edge is a real payment with its `tx_id`.
- **Disposition**: mark each reviewed alert *true positive* or *false positive* (with a note). This closes the alert and
  is the measurement data for section 10.
- **Cases**: group related alerts (e.g. one mule network) into a case, assign it, add notes.

## 7. Exporting dispositions
Dispositions live in Postgres. For a CSV:
```bash
docker compose exec -T postgres psql -U aegis -d aegis \
  -c "\copy (select * from dispositions) to stdout with csv header" > dispositions.csv
```
Or via the API: page through `GET /v1/alerts?status=closed` and read `disposition` from `GET /v1/alerts/{id}`.
Every disposition is also in the tamper-evident audit log; `GET /v1/audit/verify` confirms the log has not been altered.

## 8. Data handling and security
- Everything runs inside your network: Postgres, engine and console. Nothing calls out; you can run the host with no egress.
- Only HMAC-hashed identifiers are needed (section 3). The hashing key never leaves your side.
- The engine port is published on `127.0.0.1` only; the console listens on port 3000. Put it behind your VPN/SSO proxy
  and TLS terminator; the pilot uses a single static API key (per-user SSO is not in the pilot build).
- Secrets live only in `deploy/.env` (not committed). Rotate `AEGIS_API_KEY` by editing `.env` and `docker compose up -d`.
- Audit log: every scored transaction, alert, disposition and case change is hash-chained; `GET /v1/audit/verify`
  recomputes the chain and reports the first broken row, if any.
- End of pilot: `docker compose down -v` deletes all pilot data. Retain only the disposition export and results report if agreed.

## 9. Kubernetes (optional)
A minimal Helm chart is in `deploy/helm/aegisgraph` (external Postgres, single engine replica by design):
```bash
kubectl create secret generic aegis-secrets \
  --from-literal=api-key=... --from-literal=database-url=postgres://...
helm install aegis deploy/helm/aegisgraph --set existingSecret=aegis-secrets \
  --set engine.image.repository=<registry>/aegisgraph/engine \
  --set console.image.repository=<registry>/aegisgraph/console
```

## 10. Success criteria for the 14-day PoC
Agree the numbers at kickoff; suggested starting points are in brackets.

| Criterion | How it is measured | Suggested target |
|---|---|---|
| Recall on known-bad cases | Share of your confirmed mule accounts / APP-fraud payments in the replay period that have at least one alert on or before the payment | [≥ 70 %, and higher than your current system on the same cases] |
| Alert volume | Alerts per day (score ≥ `review`) at the final config, vs. analyst capacity | [within existing team capacity] |
| False-positive rate | Share of a reviewed sample of alerts dispositioned `false_positive` | [lower than current rules engine] |
| Latency | p99 of `aegis_score_latency_us` from `GET /metrics` during replay, plus end-to-end `POST /v1/score` round-trip from your network | [fits inside your pre-clearance budget] |
| Explainability | Analysts can state why each sampled alert fired from the console alone | [yes for every sampled alert] |
| Operability | Engine restart rebuilds state; `GET /v1/audit/verify` returns `ok: true` at end of pilot | [both pass] |

Benchmark figures on public data are in [`bench/report.md`](../bench/report.md); your own replay is the number that counts.
