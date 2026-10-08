# aegis-bench

Benchmark + replay tool for AegisGraph. Run all commands from the repo root.
Reports go to `bench/report.md` (override with `--report`). Datasets live in `bench/data/` (gitignored).

## 1. Synthetic benchmark (no external data)

Deterministic (same `--seed` => byte-identical output). Background traffic: payroll hubs (monthly/weekly BACS),
card spend to merchants with Pareto popularity, P2P inside social circles, monthly bills, ~5% shared household devices.
Injected, labelled typologies: `CYCLE`, `FAN-IN`, `PASS-THROUGH`, `FAN-OUT`, `SCATTER-GATHER`, `SHARED-DEVICE`, `VELOCITY`.

```sh
# Generate in memory and run the engine (~2.6M txns)
cargo run -p aegis-bench --release -- synth --accounts 50000 --days 30 --seed 42 --rings-per-type 50

# With a tuned engine config
cargo run -p aegis-bench --release -- synth --accounts 50000 --days 30 --config config.toml

# Write to files instead: txns (CSV, or JSONL if the path ends in .jsonl) + <name>.labels.csv
cargo run -p aegis-bench --release -- synth --accounts 50000 --days 30 --out bench/data/synth.csv

# Run the engine over any txn file (CSV/JSONL) with optional labels (tx_id,typology,attempt)
cargo run -p aegis-bench --release -- run --input bench/data/synth.csv --labels bench/data/synth.labels.csv
```

## 2. IBM AML dataset (HI-Small)

"Realistic Synthetic Financial Transactions for Anti-Money Laundering" (Altman et al., IBM). Needs a Kaggle account
and an API token in `~/.kaggle/kaggle.json` (Kaggle -> Settings -> API -> Create New Token).

```sh
pip install kaggle
kaggle datasets download -d ealtman2019/ibm-transactions-for-anti-money-laundering-aml -p bench/data
cd bench/data && unzip ibm-transactions-for-anti-money-laundering-aml.zip HI-Small_Trans.csv HI-Small_Patterns.txt && cd ../..

cargo run -p aegis-bench --release -- ibm \
  --trans bench/data/HI-Small_Trans.csv --patterns bench/data/HI-Small_Patterns.txt
# quick look: add --limit 500000
```

Mapping: account id = `<bank>-<account>`; amount = `Amount Paid` (minor units); currency name -> ISO code;
`Payment Format` ACH->bacs, Wire->chaps, Credit Card->card, else other; `tx_id` = `ibm-<row index>`.
Rows are streamed and sorted by timestamp through a bounded reorder heap (`--reorder`, default 1M rows).
Laundering rows not in any pattern block are scored as typology `UNATTRIBUTED` (txn-level only).

## Metrics

- Latency: wall time of each `Engine::process()` call, hdrhistogram p50/p90/p99/p99.9/max; engine TPS and wall TPS.
- Flagged = decision `review` or `block`.
- Txn recall = flagged laundering txns / laundering txns. Attempt recall = attempts with at least one flagged txn.
  The first txns of a pattern (e.g. inflows to a pass-through mule) are often undetectable at the time they happen,
  so attempt recall is the better measure of "would an analyst see this ring".
- Precision = flagged laundering / all flagged. Alert rate = flagged / all. Per-rule hit counts.

## 3. Replay against a running server (design-partner tool)

```sh
export AEGIS_API_KEY=...   # or --api-key
# Async batch ingest: POST /v1/transactions (max 10 000 per batch)
cargo run -p aegis-bench --release -- replay --target http://localhost:8080 --input bench/data/synth.csv --batch 1000
# Throttled
cargo run -p aegis-bench --release -- replay --target http://localhost:8080 --input export.jsonl --rps 500
# Synchronous pre-clearance: POST /v1/score per txn, end-to-end HTTP latency histogram
cargo run -p aegis-bench --release -- replay --target http://localhost:8080 --input export.jsonl --sync
```

Input: JSONL (one `Txn` per line, see `docs/api.md`) or CSV with `Txn` field names as headers (the format `synth --out` writes).
