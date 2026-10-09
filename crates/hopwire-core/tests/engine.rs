mod common;

use common::*;
use hopwire_core::{
    Config, Decision, EdgeKind, Engine, Finding, MlScorer, NodeKind, Rule, Txn, Verdict,
};

#[test]
fn eviction_forgets_old_edges_and_reclaims() {
    let mut e = Engine::new(Config {
        window_ms: HOUR,
        ..Config::default()
    });
    for i in 0..1_000 {
        let mut t = tx(
            &format!("t{i}"),
            i,
            100,
            &format!("a{}", i % 50),
            &format!("b{}", i % 70),
        );
        t.device_id = Some(format!("d{}", i % 10));
        e.ingest(&t);
    }
    let before = e.stats();
    assert!(before.edges >= 1_000, "{before:?}");
    // Two windows later, enough traffic for the round-robin sweep to visit every node.
    for i in 0..200 {
        e.ingest(&tx(&format!("late{i}"), 3 * HOUR + i, 100, "x", "y"));
    }
    let after = e.stats();
    assert_eq!(after.edges, 200, "{after:?}");
    assert_eq!(after.processed, 1_200);
    assert_eq!(after.watermark_ms, T0 + 3 * HOUR + 199);
    // Old edges are gone from detectors' view too: the old cycle leg no longer closes.
    assert!(e.subgraph("a0", 2, 100).edges.is_empty());
}

#[test]
fn evicted_cycle_leg_not_detected() {
    let mut cfg = only(Rule::Cycle);
    cfg.window_ms = HOUR;
    let mut e = Engine::new(cfg);
    e.process(&tx("1", 0, 100_000, "a", "b"));
    e.process(&tx("2", 2 * HOUR, 100_000, "b", "c"));
    assert!(!has(
        &e.process(&tx("3", 2 * HOUR + MIN, 100_000, "c", "a")),
        Rule::Cycle
    ));
}

#[test]
fn decision_bands_and_noisy_or() {
    // Single 2-hop cycle with identical amounts and zero span: score 1.0 * weight 0.9 -> 0.9 -> Block.
    let mut e = Engine::new(only(Rule::Cycle));
    e.process(&tx("1", 0, 1_000, "a", "b"));
    let v = e.process(&tx("2", 0, 1_000, "b", "a"));
    assert!((v.score - 0.9).abs() < 1e-9, "{}", v.score);
    assert_eq!(v.decision, Decision::Block);

    // Same with block band raised: Review.
    let mut cfg = only(Rule::Cycle);
    cfg.bands.block = 0.95;
    let mut e = Engine::new(cfg);
    e.process(&tx("1", 0, 1_000, "a", "b"));
    assert_eq!(
        e.process(&tx("2", 0, 1_000, "b", "a")).decision,
        Decision::Review
    );

    // Clean txn: Allow, score 0.
    let v = Engine::new(Config::default()).process(&tx("x", 0, 1_000, "p", "q"));
    assert_eq!((v.score, v.decision), (0.0, Decision::Allow));
    assert!(v.findings.is_empty());
}

struct FixedMl(f64);

impl MlScorer for FixedMl {
    fn score(&mut self, txn: &Txn) -> Option<Finding> {
        Some(Finding {
            rule: Rule::Ml,
            score: self.0,
            reason: format!("model on {}", txn.tx_id),
            path: vec![],
        })
    }
}

#[test]
fn ml_scorer_joins_noisy_or_and_findings_sorted() {
    let mut e = Engine::new(only(Rule::Cycle));
    e.set_ml_scorer(Box::new(FixedMl(0.5)));
    e.process(&tx("1", 0, 1_000, "a", "b"));
    let v = e.process(&tx("2", 0, 1_000, "b", "a"));
    // 1 - (1 - 0.9)(1 - 0.5) = 0.95
    assert!((v.score - 0.95).abs() < 1e-9, "{}", v.score);
    assert_eq!(
        v.findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
        [Rule::Cycle, Rule::Ml]
    );
}

#[test]
fn verdict_serde_roundtrip_lowercase() {
    let mut e = Engine::new(Config::default());
    e.process(&tx("1", 0, 1_000, "a", "b"));
    let v = e.process(&tx("2", 0, 1_000, "b", "a"));
    let json = serde_json::to_string(&v).unwrap();
    assert!(json.contains(r#""decision":"block""#), "{json}");
    assert!(
        json.contains(r#""rule":"cycle""#) && json.contains(r#""kind":"payment""#),
        "{json}"
    );
    let back: Verdict = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v);
    let rules =
        serde_json::to_string(&[Rule::MuleFanIn, Rule::PassThrough, Rule::SharedIdentifier])
            .unwrap();
    assert_eq!(
        rules,
        r#"["mule_fan_in","pass_through","shared_identifier"]"#
    );
}

#[test]
fn subgraph_bfs_hubs_and_limits() {
    let mut e = Engine::new(Config {
        hub_degree: 6,
        ..Config::default()
    });
    let mut t = tx("1", 0, 500, "a", "b");
    t.device_id = Some("d".into());
    t.ip = Some("1.2.3.4".into());
    e.ingest(&t);
    e.ingest(&tx("2", 1, 500, "b", "c"));
    e.ingest(&tx("3", 2, 500, "c", "far"));
    for i in 0..6 {
        e.ingest(&tx(&format!("h{i}"), 3, 100, "hub", &format!("leaf{i}")));
    }
    e.ingest(&tx("4", 4, 500, "a", "hub"));

    assert_eq!(e.subgraph("nope", 2, 100), Default::default());

    let sg = e.subgraph("a", 2, 100);
    let ids: Vec<_> = sg.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids[0], "a");
    for want in ["b", "c", "device:d", "ip:1.2.3.4", "hub"] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    // Hub listed but not expanded; 3 hops away not reached.
    assert!(
        !ids.iter().any(|i| i.starts_with("leaf") || *i == "far"),
        "{ids:?}"
    );
    let hub = sg.nodes.iter().find(|n| n.id == "hub").unwrap();
    assert!(hub.hub && hub.kind == NodeKind::Account);
    assert_eq!(
        sg.nodes.iter().find(|n| n.id == "ip:1.2.3.4").unwrap().kind,
        NodeKind::Ip
    );
    let uses = sg.edges.iter().find(|p| p.kind == EdgeKind::Uses).unwrap();
    assert_eq!(
        (uses.from.as_str(), uses.amount_minor, uses.tx_id.as_str()),
        ("a", 0, "1")
    );
    // Deterministic.
    assert_eq!(sg, e.subgraph("a", 2, 100));
    // Edge cap.
    assert_eq!(e.subgraph("a", 2, 2).edges.len(), 2);
    // Hub as start is expanded.
    assert!(e.subgraph("hub", 1, 100).nodes.len() >= 7);
}

#[test]
fn config_example_parses_to_defaults() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config.example.toml");
    let s = std::fs::read_to_string(path).unwrap();
    assert_eq!(Config::from_toml(&s).unwrap(), Config::default());
    // Partial file keeps defaults for the rest.
    let c = Config::from_toml("hub_degree = 7\n[cycle]\nmax_hops = 3\n").unwrap();
    assert_eq!(
        (c.hub_degree, c.cycle.max_hops, c.cycle.budget),
        (7, 3, Config::default().cycle.budget)
    );
}

/// Self-transfers carry no counterparty: always allow, never add edges, and never
/// let a later outflow look like pass-through of the "inflow" they would create.
#[test]
fn self_transfers_are_inert() {
    let mut e = Engine::new(Config::default());
    let v = e.process(&tx("s1", 0, 500_000, "a", "a"));
    assert_eq!(v.decision, Decision::Allow);
    assert!(v.findings.is_empty());
    assert_eq!(e.stats().edges, 0);
    assert_eq!(e.stats().processed, 1);
    let v = e.process(&tx("out", MIN, 490_000, "a", "b"));
    assert!(
        !v.findings.iter().any(|f| f.rule == Rule::PassThrough),
        "{v:#?}"
    );
}
