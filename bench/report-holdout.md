# AegisGraph benchmark report

| | |
|---|---|
| Date | 2026-10-08 |
| Dataset | synthetic (accounts=120000, days=70, seed=7, rings_per_type=40) |
| Transactions | 7141673 scored over 34.7 days |
| Warm-up | 7146704 earlier txns (35 days) ingested as history, not scored |
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
| 356494 | 283842 | 2.3 | 3.9 | 9.9 | 23.6 | 5914.6 | 2.8 |

Final graph: 233871 nodes, 723380 edges, 123 hubs (window 72 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 2814 | 3210 | 2816 | 394 | 0.0% | 61.3% | 69.9% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| CYCLE | 23 | 100.0% | 68 | 38.2% | cycle:23, pass_through:9 |
| FAN-IN | 15 | 100.0% | 357 | 64.4% | mule_fan_in:282, fan_out:27 |
| FAN-OUT | 21 | 100.0% | 462 | 59.1% | fan_out:273 |
| PASS-THROUGH | 20 | 100.0% | 62 | 37.1% | pass_through:20, fan_out:3 |
| SCATTER-GATHER | 18 | 100.0% | 410 | 48.3% | mule_fan_in:115, pass_through:121, fan_out:45 |
| SHARED-DEVICE | 23 | 91.3% | 467 | 69.2% | cycle:11, pass_through:4, shared_identifier:374 |
| VELOCITY | 22 | 100.0% | 988 | 90.6% | fan_out:895, velocity:659 |

### Rule hits

| Rule | Txns with finding | On laundering txns | On flagged txns |
|---|---|---|---|
| cycle | 1992 | 34 | 306 |
| mule_fan_in | 397 | 397 | 308 |
| pass_through | 528 | 154 | 347 |
| fan_out | 1434 | 1243 | 1395 |
| velocity | 660 | 659 | 660 |
| shared_identifier | 4300 | 374 | 947 |

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
