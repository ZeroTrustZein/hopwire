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
| 108963 | 91321 | 8.5 | 13.4 | 22.6 | 100.6 | 4251.6 | 9.2 |

Final graph: 99305 nodes, 313798 edges, 66 hubs (window 72 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 6868 | 2196946 | 2196831 | 115 | 84.9% | 0.1% | 16.0% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| CYCLE | 50 | 100.0% | 156 | 41.0% | cycle:50, pass_through:26 |
| FAN-IN | 50 | 90.0% | 1054 | 43.5% | mule_fan_in:704, pass_through:3 |
| FAN-OUT | 50 | 12.0% | 1049 | 1.2% | fan_out:599, velocity:19 |
| PASS-THROUGH | 50 | 100.0% | 141 | 35.5% | pass_through:50 |
| SCATTER-GATHER | 50 | 100.0% | 1154 | 34.4% | mule_fan_in:227, pass_through:348, fan_out:135, velocity:75 |
| SHARED-DEVICE | 50 | 62.0% | 1064 | 4.6% | cycle:36, pass_through:13, shared_identifier:852 |
| VELOCITY | 50 | 6.0% | 2250 | 3.1% | fan_out:122, velocity:1254 |

### Rule hits

| Rule | Txns with finding | On laundering txns | On flagged txns |
|---|---|---|---|
| cycle | 837 | 86 | 809 |
| mule_fan_in | 2097428 | 931 | 2086604 |
| pass_through | 1502 | 440 | 1490 |
| fan_out | 111193 | 856 | 109170 |
| velocity | 110338 | 1348 | 109084 |
| shared_identifier | 1428 | 852 | 525 |

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
weight = 0.6
window_ms = 86400000
min_senders = 8
young_account_ms = 2592000000

[pass_through]
enabled = true
weight = 0.7
within_ms = 3600000
min_ratio = 0.8
min_inflow_minor = 10000

[fan_out]
enabled = true
weight = 0.5
window_ms = 86400000
min_payees = 10

[velocity]
enabled = true
weight = 0.4
window_ms = 3600000
max_count = 20
max_sum_minor = 5000000

[shared_id]
enabled = true
weight = 0.5
min_accounts = 4

[bands]
review = 0.5
block = 0.85
```
