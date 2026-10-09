# Hopwire benchmark report

| | |
|---|---|
| Date | 2026-10-09 |
| Dataset | IBM AML bench/data/HI-Small_Trans.csv |
| Transactions | 1346673 scored over 10.7 days |
| Warm-up | 3731672 earlier txns (7 days) ingested as history, not scored |
| Engine | hopwire-core 0.1.0 (in-process, single thread) |
| CPU | 12th Gen Intel(R) Core(TM) i9-12900K (24 logical cores) |
| RAM | 31.8 GiB |
| OS | Windows 11 Pro |
| rustc | rustc 1.98.0 (88d9e12ae 2026-08-18) |
| Build | release |

## Throughput and latency

Latency = wall time of one `Engine::process()` call (ingest + all detectors).

| Engine TPS | Wall TPS (incl. data gen/parse) | p50 µs | p90 µs | p99 µs | p99.9 µs | max µs | mean µs |
|---|---|---|---|---|---|---|---|
| 22248 | 19960 | 40.6 | 60.0 | 103.6 | 202.6 | 42074.1 | 45.0 |

Final graph: 422734 nodes, 3230646 edges, 2 hubs (window 192 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 2150 | 110110 | 108981 | 1129 | 8.2% | 1.5% | 76.9% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| BIPARTITE | 21 | 95.2% | 90 | 87.8% | cycle:11, pass_through:11, velocity:8, ml:85 |
| CYCLE | 30 | 96.7% | 136 | 86.0% | cycle:23, pass_through:43, fan_out:1, velocity:18, ml:119 |
| FAN-IN | 24 | 95.8% | 157 | 87.9% | cycle:7, pass_through:6, velocity:3, ml:148 |
| FAN-OUT | 24 | 91.7% | 160 | 92.5% | cycle:14, pass_through:9, fan_out:112, velocity:14, ml:153 |
| GATHER-SCATTER | 40 | 97.5% | 435 | 92.6% | cycle:30, pass_through:24, fan_out:170, velocity:36, ml:406 |
| RANDOM | 21 | 100.0% | 97 | 80.4% | cycle:12, pass_through:24, velocity:14, ml:84 |
| SCATTER-GATHER | 29 | 93.1% | 299 | 84.6% | cycle:20, pass_through:23, fan_out:81, velocity:35, ml:265 |
| STACK | 27 | 100.0% | 194 | 78.9% | cycle:14, pass_through:21, velocity:24, ml:159 |
| UNATTRIBUTED | 0 | - | 582 | 48.8% | cycle:46, pass_through:110, velocity:9, ml:318 |

### Rule hits

Rule precision = laundering share of txns carrying that finding. Lift = rule precision / base rate (0.2%).

| Rule | Txns with finding | On laundering txns | On flagged txns | Rule precision | Lift |
|---|---|---|---|---|---|
| cycle | 2286 | 177 | 1466 | 7.7% | 48x |
| pass_through | 151378 | 271 | 29671 | 0.2% | 1x |
| fan_out | 2376 | 364 | 443 | 15.3% | 96x |
| velocity | 813 | 161 | 759 | 19.8% | 124x |
| ml | 194461 | 1737 | 109778 | 0.9% | 6x |

## Config

```toml
window_ms = 691200000
history_ms = 691200000
hub_degree = 500

[cycle]
enabled = true
weight = 0.9
max_hops = 4
budget = 2000
max_span_ms = 691200000

[fan_in]
enabled = true
weight = 0.58
window_ms = 345600000
min_senders = 5
young_account_ms = 2592000000
min_new_senders = 5

[pass_through]
enabled = true
weight = 0.45
within_ms = 21600000
min_ratio = 0.8
min_inflow_minor = 10000

[fan_out]
enabled = true
weight = 0.6
window_ms = 345600000
min_payees = 5
young_payee_ms = 2592000000

[velocity]
enabled = true
weight = 0.7
window_ms = 3600000
max_count = 15
max_sum_minor = 50000000
young_payee_ms = 2592000000

[shared_id]
enabled = true
weight = 0.85
min_accounts = 4

[bands]
review = 0.5
block = 0.85
```
