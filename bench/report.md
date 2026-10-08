# AegisGraph benchmark report

| | |
|---|---|
| Date | 2026-10-08 |
| Dataset | synthetic (accounts=50000, days=65, seed=42, rings_per_type=50) |
| Transactions | 2581639 scored over 29.7 days |
| Warm-up | 2997409 earlier txns (35 days) ingested as history, not scored |
| Engine | aegis-core 0.1.0 (in-process, single thread) |
| CPU | 12th Gen Intel(R) Core(TM) i9-12900K (24 logical cores) |
| RAM | 31.8 GiB |
| OS | Windows 11 Pro |
| rustc | rustc 1.98.0 (88d9e12ae 2026-08-18) |
| Build | release |

## Throughput and latency

Latency = wall time of one `Engine::process()` call (ingest + all detectors).

| Engine TPS | Wall TPS (incl. data gen/parse) | p50 µs | p90 µs | p99 µs | p99.9 µs | max µs | mean µs |
|---|---|---|---|---|---|---|---|
| 400453 | 309547 | 2.0 | 3.5 | 9.4 | 43.6 | 2842.6 | 2.5 |

Final graph: 99305 nodes, 317606 edges, 67 hubs (window 72 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 2947 | 2286 | 1950 | 336 | 0.1% | 85.0% | 65.9% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| CYCLE | 29 | 100.0% | 91 | 39.6% | cycle:29, pass_through:13 |
| FAN-IN | 23 | 100.0% | 501 | 61.1% | mule_fan_in:386, pass_through:1, fan_out:34 |
| FAN-OUT | 26 | 100.0% | 523 | 55.3% | fan_out:289 |
| PASS-THROUGH | 22 | 100.0% | 58 | 41.4% | pass_through:22, fan_out:2 |
| SCATTER-GATHER | 26 | 100.0% | 634 | 51.1% | mule_fan_in:187, pass_through:190, fan_out:85 |
| SHARED-DEVICE | 13 | 92.3% | 265 | 60.4% | cycle:6, pass_through:5, shared_identifier:203 |
| VELOCITY | 20 | 100.0% | 875 | 91.9% | fan_out:804, velocity:576 |

### Rule hits

| Rule | Txns with finding | On laundering txns | On flagged txns |
|---|---|---|---|
| cycle | 757 | 35 | 154 |
| mule_fan_in | 573 | 573 | 436 |
| pass_through | 383 | 231 | 316 |
| fan_out | 1359 | 1214 | 1354 |
| velocity | 578 | 576 | 578 |
| shared_identifier | 801 | 203 | 159 |

## Config

```toml
window_ms = 259200000
history_ms = 3024000000
hub_degree = 500

[cycle]
enabled = true
weight = 0.9
max_hops = 4
budget = 2000
max_span_ms = 259200000

[fan_in]
enabled = true
weight = 0.58
window_ms = 86400000
min_senders = 6
young_account_ms = 2592000000
min_new_senders = 6

[pass_through]
enabled = true
weight = 0.7
within_ms = 3600000
min_ratio = 0.8
min_inflow_minor = 10000

[fan_out]
enabled = true
weight = 0.6
window_ms = 86400000
min_payees = 10
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
