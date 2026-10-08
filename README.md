# AegisGraph

Real-time graph AML and transaction monitoring engine, self-hostable in-VPC.

- Cycle, mule fan-in, rapid pass-through, and velocity detection (target: p99 < 1 ms per txn; not yet benchmarked).
- In-memory time-windowed graph in Rust; no external graph database needed.
- Bounded BFS with degree caps to prevent supernode latency spikes.
- BSL 1.1 source-available core; see `LICENSE`. Commercial enterprise modules live in `ee/`.

## Architecture
- `crates/aegis-core`: in-memory graph, detectors, scoring arbiter (zero IO).
- `crates/aegis-server`: axum HTTP API, Postgres state, audit log actor.
- `crates/aegis-bench`: replay benchmark harness (IBM AMLSim / Kaggle datasets).
- `console/`: React + Cytoscape investigator UI (Phase 4).
- `ee/`: enterprise modules (SSO, automated SAR filing, consortium risk).

## Quickstart
```bash
cargo test --workspace
```
