# Hopwire

Real-time, in-memory graph anti-money laundering (AML) and transaction monitoring engine in Rust.

Scores financial payments against a live, time-windowed graph of recent payments and shared identifiers (devices, IPs, customer hashes) **before settlement clearance**. Every alert outputs the exact payment path that triggered it, providing verifiable explanations rather than black-box opacity.

```
   Payment (JSON / ISO 20022 pacs.008)
                  │
                  ▼
   ┌──────────────────────────────┐
   │ In-Memory Sliding Graph(RAM) │
   │ - 72h Payment Edges (Deq)    │
   │ - Counterparty LRU Cache     │
   └──────────────────────────────┘
      │                        │
      ▼                        ▼
┌───────────────────┐    ┌───────────────────┐
│ Deterministic BFS │    │  Continuous-Time  │
│ - Bounded Cycles  │    │  Neural Path      │
│ - Supernode Caps  │    │ - GRU Node Memory │
│ - Mule Fan-In     │    │ - Harmonic Time   │
│ - Pass-Through    │    │ - Embedded ONNX   │
└───────────────────┘    └───────────────────┘
      │                        │
      └───────────┬────────────┘
                  ▼
      Ensemble Scoring & Path Verdict
      Allow / Review / Block + Subgraph
```

## Benchmarks & Performance

Evaluated on an Intel Core i9-12900K (single thread, release build):

| Benchmark | Transactions | p99 Latency | Throughput | Precision | Attempt Recall | Report |
|---|---|---|---|---|---|---|
| Synthetic Primary (35d warm-up) | 2.6M | **9.2 µs** | 376k/s | **88.5%** | 99% | [report.md](bench/report.md) |
| Synthetic Holdout (unseen seed) | 7.1M | **13.8 µs** | 288k/s | **65.5%** | 99% | [report-holdout.md](bench/report-holdout.md) |
| IBM AML HI-Small (Altman et al.) | 1.3M | **103.6 µs** | 22k/s | 1.5% | **96.3%** | [report-ibm.md](bench/report-ibm.md) |
| Full Enterprise Scale | 77M (1M accts) | **14.2 µs** | 224k/s | 55.4% | peak RAM **5.7 GB** | [scale report](bench/README.md) |

*Full methodology, dataset links, and reproduction scripts: [`bench/README.md`](bench/README.md).*

## Features

- **Sub-Millisecond Bounded Traversal**: Evaluates 2–4 hop cyclic layering and pass-through conduits in RAM without recursive relational joins.
- **Supernode Degree Ceilings**: Nodes exceeding degree thresholds (hubs like payroll providers or merchants) are capped during traversal, eliminating latency spikes.
- **Counterparty Memory**: Per-account LRU cache distinguishes recurring counterparties (salaries, standing orders) from new dispersal rings, cutting payroll false positives by >80%.
- **Embedded Neural Inference**: Optional continuous-time dynamic graph (TGN) inference executed directly inside Rust via `tract-onnx` with zero Python sidecar overhead.
- **Native ISO 20022**: Zero-copy parser for `pacs.008.001.10` credit transfers and generator for `pacs.002` payment status rejections.
- **Tamper-Evident Audit Ledger**: Every transaction, verdict, case note, and disposition is cryptographically hash-chained via SHA-256 (`GET /v1/audit/verify`).
- **Investigator Console**: React 19 + Cytoscape visualization with interactive chronological fund flow playback, SAR narrative drafting, and model telemetry.

## Using as a Rust Library (`hopwire-core`)

```rust
use hopwire_core::{Config, Engine, Scheme, Txn, Decision};

fn main() {
    let mut engine = Engine::new(Config::default());

    let txn = Txn {
        tx_id: "tx-001".into(),
        ts_ms: 1_700_000_000_000,
        amount_minor: 250_000, // £2,500.00
        currency: "GBP".into(),
        debtor: "acc-alice".into(),
        creditor: "acc-bob".into(),
        scheme: Scheme::Fps,
        device_id: Some("dev-alice".into()),
        ip: Some("192.168.1.10".into()),
        debtor_customer_hash: None,
        creditor_customer_hash: None,
        creditor_account_opened_at_ms: None,
    };

    let verdict = engine.process(&txn);
    println!("Score: {:.2}, Decision: {:?}", verdict.score, verdict.decision);
    for finding in &verdict.findings {
        println!(" - [{:?}] {}", finding.rule, finding.reason);
    }
}
```

## Running the Standalone Platform

### Docker Compose
```bash
cd deploy
cp .env.example .env        # set your secure API key & DB password
docker compose up -d --build
```
- **Investigator Console**: http://localhost:3000
- **Engine API**: http://localhost:8080

### Direct Cargo & Dev Run
```bash
# Run unit and integration test suite (starts embedded Postgres automatically)
cargo test --workspace

# Run local development server
DATABASE_URL=postgres://user:pass@localhost:5432/hopwire HOPWIRE_API_KEY=devkey cargo run --release -p hopwire-server

# Run the console
cd console && npm ci && npm run dev
```

### Quick Score Check
```bash
curl -X POST http://localhost:8080/v1/score \
  -H "Authorization: Bearer devkey" \
  -H "Content-Type: application/json" \
  -d '{
    "tx_id": "tx-sample-1",
    "ts_ms": 1700000000000,
    "amount_minor": 500000,
    "currency": "GBP",
    "debtor": "alice",
    "creditor": "bob",
    "scheme": "fps"
  }'
```

## Workspace Crates

- [`crates/hopwire-core`](crates/hopwire-core): In-memory sliding-window graph, detectors, scoring arbiter, and ONNX runtime. Zero I/O.
- [`crates/hopwire-iso20022`](crates/hopwire-iso20022): ISO 20022 `pacs.008` XML credit transfer parser and `pacs.002` rejection generator.
- [`crates/hopwire-server`](crates/hopwire-server): Axum HTTP API, Postgres state, Redpanda/Kafka consumer, and SHA-256 audit log actor.
- [`crates/hopwire-bench`](crates/hopwire-bench): Benchmark harness (IBM AML, synthetic generator) and replay tool.
- [`console`](console): React 19 + TypeScript + Cytoscape investigator UI.

## License

Dual-licensed under either:

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT License ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
