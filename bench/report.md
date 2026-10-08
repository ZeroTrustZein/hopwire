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
| 129220 | 109621 | 7.6 | 10.6 | 15.8 | 62.8 | 6176.8 | 7.7 |

Final graph: 99305 nodes, 313798 edges, 66 hubs (window 72 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 6868 | 115165 | 7196 | 107969 | 4.5% | 2.9% | 47.9% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| CYCLE | 50 | 100.0% | 156 | 41.0% | cycle:50, pass_through:26 |
| FAN-IN | 50 | 100.0% | 1054 | 57.6% | mule_fan_in:804, pass_through:3 |
| FAN-OUT | 50 | 74.0% | 1049 | 22.9% | fan_out:599 |
| PASS-THROUGH | 50 | 100.0% | 141 | 35.5% | pass_through:50 |
| SCATTER-GATHER | 50 | 100.0% | 1154 | 37.3% | mule_fan_in:327, pass_through:348, fan_out:135 |
| SHARED-DEVICE | 50 | 96.0% | 1064 | 68.4% | cycle:36, pass_through:13, shared_identifier:852 |
| VELOCITY | 50 | 100.0% | 2250 | 52.0% | fan_out:122, velocity:1504 |

### Rule hits

| Rule | Txns with finding | On laundering txns | On flagged txns |
|---|---|---|---|
| cycle | 837 | 86 | 809 |
| mule_fan_in | 2099635 | 1131 | 2341 |
| pass_through | 1502 | 440 | 1490 |
| fan_out | 111193 | 856 | 109976 |
| velocity | 111039 | 1504 | 110705 |
| shared_identifier | 1428 | 852 | 1204 |

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

[velocity]
enabled = true
weight = 0.7
window_ms = 3600000
max_count = 15
max_sum_minor = 50000000

[shared_id]
enabled = true
weight = 0.85
min_accounts = 4

[bands]
review = 0.5
block = 0.85
```
