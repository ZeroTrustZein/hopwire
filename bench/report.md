# AegisGraph benchmark report

| | |
|---|---|
| Date | 2026-10-08 |
| Dataset | synthetic (accounts=50000, days=30, seed=42, rings_per_type=50) |
| Transactions | 2587157 over 29.7 days |
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
| 407524 | 319164 | 1.7 | 3.0 | 21.2 | 38.9 | 3477.5 | 2.5 |

Final graph: 99305 nodes, 313798 edges, 66 hubs (window 72 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 6868 | 55031 | 54119 | 912 | 2.1% | 8.6% | 68.8% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| CYCLE | 50 | 100.0% | 156 | 41.0% | cycle:50, pass_through:26 |
| FAN-IN | 50 | 100.0% | 1054 | 61.5% | mule_fan_in:804, pass_through:3, fan_out:78 |
| FAN-OUT | 50 | 100.0% | 1049 | 57.1% | fan_out:599 |
| PASS-THROUGH | 50 | 100.0% | 141 | 44.0% | pass_through:50, fan_out:12 |
| SCATTER-GATHER | 50 | 100.0% | 1154 | 49.0% | mule_fan_in:327, pass_through:348, fan_out:135 |
| SHARED-DEVICE | 50 | 96.0% | 1064 | 68.4% | cycle:36, pass_through:13, shared_identifier:852 |
| VELOCITY | 50 | 100.0% | 2250 | 91.4% | fan_out:2056, velocity:1503 |

### Rule hits

| Rule | Txns with finding | On laundering txns | On flagged txns |
|---|---|---|---|
| cycle | 837 | 86 | 811 |
| mule_fan_in | 20403 | 1131 | 1064 |
| pass_through | 1502 | 440 | 1460 |
| fan_out | 51767 | 2880 | 51444 |
| velocity | 49709 | 1503 | 49709 |
| shared_identifier | 1428 | 852 | 726 |

## Config

```toml
window_ms = 259200000
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
