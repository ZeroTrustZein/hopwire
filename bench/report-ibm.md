# AegisGraph benchmark report

| | |
|---|---|
| Date | 2026-10-08 |
| Dataset | IBM AML bench/data/HI-Small_Trans.csv |
| Transactions | 1346673 scored over 10.7 days |
| Warm-up | 3731672 earlier txns (7 days) ingested as history, not scored |
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
| 154729 | 94856 | 3.6 | 14.4 | 40.0 | 75.8 | 647.7 | 6.5 |

Final graph: 422734 nodes, 3230646 edges, 2 hubs (window 192 h).

## Detection

Flagged = decision `review` or `block`.

| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |
|---|---|---|---|---|---|---|
| 2150 | 980 | 933 | 47 | 0.1% | 20.9% | 9.5% |

### Recall by typology

Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).

| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |
|---|---|---|---|---|---|
| BIPARTITE | 21 | 42.9% | 90 | 11.1% | cycle:11, pass_through:11, velocity:8 |
| CYCLE | 30 | 40.0% | 136 | 16.9% | cycle:23, pass_through:43, fan_out:1, velocity:18 |
| FAN-IN | 24 | 12.5% | 157 | 1.9% | cycle:7, pass_through:6, velocity:3 |
| FAN-OUT | 24 | 50.0% | 160 | 15.6% | cycle:14, pass_through:9, fan_out:112, velocity:14 |
| GATHER-SCATTER | 40 | 60.0% | 435 | 9.9% | cycle:30, pass_through:24, fan_out:170, velocity:36 |
| RANDOM | 21 | 33.3% | 97 | 18.6% | cycle:12, pass_through:24, velocity:14 |
| SCATTER-GATHER | 29 | 41.4% | 299 | 15.1% | cycle:20, pass_through:23, fan_out:81, velocity:35 |
| STACK | 27 | 44.4% | 194 | 12.9% | cycle:14, pass_through:21, velocity:24 |
| UNATTRIBUTED | 0 | - | 582 | 2.2% | cycle:46, pass_through:110, velocity:9 |

### Rule hits

Rule precision = laundering share of txns carrying that finding. Lift = rule precision / base rate (0.2%).

| Rule | Txns with finding | On laundering txns | On flagged txns | Rule precision | Lift |
|---|---|---|---|---|---|
| cycle | 2286 | 177 | 282 | 7.7% | 48x |
| pass_through | 151378 | 271 | 152 | 0.2% | 1x |
| fan_out | 2376 | 364 | 74 | 15.3% | 96x |
| velocity | 813 | 161 | 704 | 19.8% | 124x |

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
