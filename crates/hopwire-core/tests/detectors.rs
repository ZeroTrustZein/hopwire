mod common;

use std::time::Instant;

use common::*;
use hopwire_core::{Config, EdgeKind, Engine, Rule};

/// Run a ring `a0 -> a1 -> ... -> a{n-1} -> a0`, one minute apart; return the closing verdict.
fn ring(e: &mut Engine, n: usize) -> hopwire_core::Verdict {
    for i in 0..n - 1 {
        let v = e.process(&tx(
            &format!("r{i}"),
            i as i64 * MIN,
            100_000,
            &format!("a{i}"),
            &format!("a{}", i + 1),
        ));
        assert!(!has(&v, Rule::Cycle), "premature cycle at leg {i}");
    }
    e.process(&tx(
        "close",
        (n - 1) as i64 * MIN,
        95_000,
        &format!("a{}", n - 1),
        "a0",
    ))
}

#[test]
fn cycles_of_2_3_4_hops_detected() {
    for n in 2..=4 {
        let mut e = Engine::new(only(Rule::Cycle));
        let v = ring(&mut e, n);
        let f = finding(&v, Rule::Cycle);
        assert_eq!(f.path.len(), n, "{n}-hop path");
        assert_eq!(f.path.last().unwrap().tx_id, "close");
        assert_eq!(f.path[0].from, "a0");
        assert!(f
            .path
            .windows(2)
            .all(|w| w[0].to == w[1].from && w[0].ts_ms <= w[1].ts_ms));
        assert!(
            f.reason.contains(&format!("{n}-hop")) && f.reason.contains("950.00 GBP"),
            "{}",
            f.reason
        );
        assert!(f.score > 0.5 && f.score <= 1.0);
    }
}

#[test]
fn cycle_of_5_hops_not_detected() {
    let mut e = Engine::new(only(Rule::Cycle));
    assert!(!has(&ring(&mut e, 5), Rule::Cycle));
}

#[test]
fn non_time_ordered_cycle_not_detected() {
    let mut e = Engine::new(only(Rule::Cycle));
    // b -> c happens BEFORE a -> b, so money cannot have flowed a -> b -> c -> a.
    e.process(&tx("1", 0, 100_000, "b", "c"));
    e.process(&tx("2", MIN, 100_000, "a", "b"));
    let v = e.process(&tx("3", 2 * MIN, 100_000, "c", "a"));
    assert!(!has(&v, Rule::Cycle));
}

#[test]
fn cycle_outside_span_not_detected() {
    let mut cfg = only(Rule::Cycle);
    cfg.cycle.max_span_ms = HOUR;
    let mut e = Engine::new(cfg);
    e.process(&tx("1", 0, 100_000, "a", "b"));
    e.process(&tx("2", 30 * MIN, 100_000, "b", "c"));
    let v = e.process(&tx("3", 2 * HOUR, 100_000, "c", "a"));
    assert!(!has(&v, Rule::Cycle));
}

#[test]
fn cycle_through_hub_not_detected() {
    let mut cfg = only(Rule::Cycle);
    cfg.hub_degree = 10;
    let mut e = Engine::new(cfg);
    for i in 0..20 {
        e.process(&tx(&format!("h{i}"), 0, 100, &format!("x{i}"), "hub"));
    }
    e.process(&tx("1", MIN, 100_000, "a", "hub"));
    e.process(&tx("2", 2 * MIN, 100_000, "hub", "c"));
    let v = e.process(&tx("3", 3 * MIN, 100_000, "c", "a"));
    assert!(!has(&v, Rule::Cycle));
    assert!(e.stats().hubs >= 1);
    // Same shape through a non-hub is caught.
    e.process(&tx("4", 4 * MIN, 100_000, "a", "m"));
    e.process(&tx("5", 5 * MIN, 100_000, "m", "c"));
    assert!(has(
        &e.process(&tx("6", 6 * MIN, 100_000, "c", "a")),
        Rule::Cycle
    ));
}

#[test]
fn cycle_budget_caps_dense_search() {
    let mut cfg = only(Rule::Cycle);
    cfg.cycle.budget = 500;
    cfg.hub_degree = 10_000;
    let mut e = Engine::new(cfg);
    // Dense layered DAG: v -> 60 nodes -> 60 nodes, all edges in time order, none reach u.
    let w = 60;
    for i in 0..w {
        e.ingest(&tx(&format!("v{i}"), 0, 100, "v", &format!("l1_{i}")));
    }
    for i in 0..w {
        for j in 0..w {
            e.ingest(&tx(
                &format!("m{i}_{j}"),
                MIN,
                100,
                &format!("l1_{i}"),
                &format!("l2_{j}"),
            ));
        }
    }
    // The only cycle v -> l1_last -> z -> u sits behind ~3.5k dense edge visits.
    e.ingest(&tx("z", MIN, 100, &format!("l1_{}", w - 1), "z"));
    e.ingest(&tx("back", 2 * MIN, 100, "z", "u"));
    let start = Instant::now();
    let v = e.process(&tx("close", 3 * MIN, 100, "u", "v"));
    assert!(start.elapsed().as_millis() < 50);
    assert!(!has(&v, Rule::Cycle));
    // With a big enough budget the 4-hop cycle u -> v -> l1_last -> z -> u is found.
    let mut cfg = only(Rule::Cycle);
    cfg.cycle.budget = 1_000_000;
    cfg.hub_degree = 10_000;
    let mut e2 = Engine::new(cfg);
    for i in 0..w {
        e2.ingest(&tx(&format!("v{i}"), 0, 100, "v", &format!("l1_{i}")));
        for j in 0..w {
            e2.ingest(&tx(
                &format!("m{i}_{j}"),
                MIN,
                100,
                &format!("l1_{i}"),
                &format!("l2_{j}"),
            ));
        }
    }
    e2.ingest(&tx("z", MIN, 100, &format!("l1_{}", w - 1), "z"));
    e2.ingest(&tx("back", 2 * MIN, 100, "z", "u"));
    assert_eq!(
        finding(
            &e2.process(&tx("close", 3 * MIN, 100, "u", "v")),
            Rule::Cycle
        )
        .path
        .len(),
        4
    );
}

#[test]
fn fan_in_positive_and_young_boost() {
    let mut cfg = only(Rule::MuleFanIn);
    cfg.fan_in.min_new_senders = 1;
    let mut e = Engine::new(cfg);
    for i in 0..5 {
        let v = e.process(&tx(
            &format!("f{i}"),
            i * MIN,
            20_000,
            &format!("s{i}"),
            "mule",
        ));
        assert!(!has(&v, Rule::MuleFanIn));
    }
    let mut t = tx("f7", 8 * MIN, 20_000, "s7", "mule");
    let v_old = Engine::new(only(Rule::MuleFanIn)).process(&t);
    assert!(!has(&v_old, Rule::MuleFanIn));
    let plain = e.process(&t);
    let f = finding(&plain, Rule::MuleFanIn);
    assert_eq!(f.path.len(), 6);
    assert!(
        f.reason.contains("6 distinct senders") && f.reason.contains("1200.00 GBP"),
        "{}",
        f.reason
    );

    let mut e = Engine::new(only(Rule::MuleFanIn));
    for i in 0..7 {
        e.process(&tx(
            &format!("f{i}"),
            i * MIN,
            20_000,
            &format!("s{i}"),
            "mule",
        ));
    }
    t.creditor_account_opened_at_ms = Some(t.ts_ms - 3 * DAY);
    let young = e.process(&t);
    let fy = finding(&young, Rule::MuleFanIn);
    assert!(
        fy.score > f.score && fy.reason.contains("opened only 3d"),
        "{}",
        fy.reason
    );
}

#[test]
fn fan_in_negative_repeat_sender_and_window() {
    let mut e = Engine::new(only(Rule::MuleFanIn));
    // Many payments, but only 2 distinct senders.
    for i in 0..20 {
        let v = e.process(&tx(
            &format!("p{i}"),
            i * MIN,
            20_000,
            &format!("s{}", i % 2),
            "acct",
        ));
        assert!(!has(&v, Rule::MuleFanIn));
    }
    // 8 distinct senders, but spread over 8 days (window 24h).
    let mut e = Engine::new(only(Rule::MuleFanIn));
    for i in 0..8 {
        let v = e.process(&tx(
            &format!("d{i}"),
            i * DAY,
            20_000,
            &format!("s{i}"),
            "acct",
        ));
        assert!(!has(&v, Rule::MuleFanIn));
    }
}

#[test]
fn pass_through_positive() {
    let mut e = Engine::new(only(Rule::PassThrough));
    e.process(&tx("in1", 0, 300_000, "victim1", "mule"));
    e.process(&tx("in2", 5 * MIN, 200_000, "victim2", "mule"));
    let v = e.process(&tx("out", 15 * MIN, 450_000, "mule", "cashout"));
    let f = finding(&v, Rule::PassThrough);
    assert_eq!(f.path.len(), 3);
    assert_eq!(f.path.last().unwrap().tx_id, "out");
    assert!(
        f.reason.contains("4500.00 GBP")
            && f.reason.contains("90%")
            && f.reason.contains("5000.00 GBP"),
        "{}",
        f.reason
    );
}

#[test]
fn pass_through_negative() {
    // Forwarded too little.
    let mut e = Engine::new(only(Rule::PassThrough));
    e.process(&tx("in", 0, 500_000, "v", "acct"));
    assert!(!has(
        &e.process(&tx("out", MIN, 100_000, "acct", "x")),
        Rule::PassThrough
    ));
    // Forwarded too late.
    let mut e = Engine::new(only(Rule::PassThrough));
    e.process(&tx("in", 0, 500_000, "v", "acct"));
    assert!(!has(
        &e.process(&tx("out", 2 * HOUR, 500_000, "acct", "x")),
        Rule::PassThrough
    ));
    // Inflow below min_inflow_minor.
    let mut e = Engine::new(only(Rule::PassThrough));
    e.process(&tx("in", 0, 5_000, "v", "acct"));
    assert!(!has(
        &e.process(&tx("out", MIN, 5_000, "acct", "x")),
        Rule::PassThrough
    ));
    // No inflow at all.
    let mut e = Engine::new(only(Rule::PassThrough));
    assert!(!has(
        &e.process(&tx("out", MIN, 500_000, "acct", "x")),
        Rule::PassThrough
    ));
}

#[test]
fn fan_out_positive_and_negative() {
    let mut cfg = only(Rule::FanOut);
    cfg.fan_out.min_payees = 10;
    let mut e = Engine::new(cfg);
    for i in 0..9 {
        assert!(!has(
            &e.process(&tx(
                &format!("o{i}"),
                i * MIN,
                10_000,
                "src",
                &format!("p{i}")
            )),
            Rule::FanOut
        ));
    }
    let v = e.process(&tx("o9", 10 * MIN, 10_000, "src", "p9"));
    let f = finding(&v, Rule::FanOut);
    assert_eq!(f.path.len(), 10);
    assert!(
        f.reason.contains("10 new payees") && f.reason.contains("1000.00 GBP"),
        "{}",
        f.reason
    );

    // 30 staff repeated across three monthly payroll runs: historical payees are not novel.
    let mut e = Engine::new(only(Rule::FanOut));
    for month in 0..3 {
        for i in 0..30 {
            let v = e.process(&tx(
                &format!("pay-{month}-{i}"),
                month * 30 * DAY + i,
                50_000,
                "employer",
                &format!("staff{i}"),
            ));
            if month >= 1 {
                assert!(!has(&v, Rule::FanOut));
            }
        }
    }

    // First-time scatter to 12 payees: detected and boosted above the review band.
    let mut cfg = only(Rule::FanOut);
    cfg.fan_out.min_payees = 12;
    let mut e = Engine::new(cfg);
    for i in 0..11 {
        assert!(!has(
            &e.process(&tx(
                &format!("new{i}"),
                i,
                10_000,
                "scatter",
                &format!("new{i}")
            )),
            Rule::FanOut
        ));
    }
    let mut last = tx("new11", 12, 10_000, "scatter", "new11");
    last.creditor_account_opened_at_ms = Some(last.ts_ms - DAY);
    let v = e.process(&last);
    let f = finding(&v, Rule::FanOut);
    assert!(
        f.score >= 0.5 && f.reason.contains("12 new payees"),
        "{}",
        f.reason
    );

    // Same payee repeatedly: no fan-out.
    let mut e = Engine::new(only(Rule::FanOut));
    for i in 0..30 {
        assert!(!has(
            &e.process(&tx(&format!("o{i}"), i * MIN, 10_000, "src", "same")),
            Rule::FanOut
        ));
    }
}

#[test]
fn velocity_count_sum_and_negative() {
    let mut e = Engine::new(only(Rule::Velocity));
    for i in 0..15 {
        assert!(!has(
            &e.process(&tx(&format!("c{i}"), i * MIN, 100, "a", &format!("b{i}"))),
            Rule::Velocity
        ));
    }
    let f = finding(
        &e.process(&tx("c15", 16 * MIN, 100, "a", "b15")),
        Rule::Velocity,
    )
    .clone();
    assert!(
        f.reason.contains("16 payments to new payees"),
        "{}",
        f.reason
    );

    let mut e = Engine::new(only(Rule::Velocity));
    e.process(&tx("s1", 0, 30_000_000, "a", "b"));
    let f = finding(
        &e.process(&tx("s2", MIN, 30_000_000, "a", "c")),
        Rule::Velocity,
    )
    .clone();
    assert!(f.reason.contains("600000.00 GBP"), "{}", f.reason);

    // 21 txns spread over 21 hours (window 1h): fine.
    let mut e = Engine::new(only(Rule::Velocity));
    for i in 0..21 {
        assert!(!has(
            &e.process(&tx(&format!("h{i}"), i * HOUR, 100, "a", "b")),
            Rule::Velocity
        ));
    }
}

#[test]
fn shared_identifier_positive_and_negative() {
    let mut e = Engine::new(only(Rule::SharedIdentifier));
    for i in 0..3 {
        let mut t = tx(
            &format!("d{i}"),
            i * MIN,
            1_000,
            &format!("acct{i}"),
            "shop",
        );
        t.device_id = Some("dev1".into());
        assert!(!has(&e.process(&t), Rule::SharedIdentifier));
    }
    let mut t = tx("d3", 4 * MIN, 1_000, "acct3", "shop");
    t.device_id = Some("dev1".into());
    let v = e.process(&t);
    let f = finding(&v, Rule::SharedIdentifier);
    assert!(f
        .path
        .iter()
        .all(|p| p.kind == EdgeKind::Uses && p.to == "device:dev1"));
    assert_eq!(f.path.len(), 4);
    assert!(
        f.reason.contains("device:dev1") && f.reason.contains("4 distinct accounts"),
        "{}",
        f.reason
    );

    // One account reusing its device many times: no finding.
    let mut e = Engine::new(only(Rule::SharedIdentifier));
    for i in 0..10 {
        let mut t = tx(
            &format!("s{i}"),
            i * MIN,
            1_000,
            "acct",
            &format!("shop{i}"),
        );
        t.device_id = Some("dev1".into());
        t.ip = Some("10.0.0.1".into());
        assert!(!has(&e.process(&t), Rule::SharedIdentifier));
    }
}

#[test]
fn disabled_detectors_emit_nothing() {
    let mut cfg = only(Rule::Cycle);
    cfg.cycle.enabled = false;
    let mut e = Engine::new(cfg);
    let v = ring(&mut e, 3);
    assert!(v.findings.is_empty());
    assert_eq!(v.score, 0.0);
}

// --- Precision guards: everyday behaviour that must NOT alert --------------------

/// Employer pays the same 40 staff monthly. Month 1 is new (may alert); month 2,
/// with counterparty memory surviving the 72h window, must be silent.
#[test]
fn recurring_payroll_is_silent_after_first_run() {
    let mut e = Engine::new(Config::default());
    let staff = 40;
    let run = |e: &mut Engine, month: i64| {
        (0..staff)
            .map(|s| {
                let mut t = tx(
                    &format!("p{month}-{s}"),
                    month * 30 * DAY + s * MIN,
                    250_000,
                    "employer",
                    &format!("staff{s}"),
                );
                t.scheme = hopwire_core::Scheme::Bacs;
                e.process(&t)
            })
            .collect::<Vec<_>>()
    };
    run(&mut e, 0);
    let second = run(&mut e, 1);
    assert!(
        second
            .iter()
            .all(|v| v.decision == hopwire_core::Decision::Allow),
        "{:#?}",
        second
            .iter()
            .find(|v| v.decision != hopwire_core::Decision::Allow)
    );
}

/// First-time scatter to 12 never-seen payees still alerts.
#[test]
fn first_time_scatter_alerts() {
    let mut e = Engine::new(only(Rule::FanOut));
    let mut last = None;
    for i in 0..12 {
        last = Some(e.process(&tx(
            &format!("s{i}"),
            i * MIN,
            90_000,
            "mule",
            &format!("drop{i}"),
        )));
    }
    assert!(has(&last.unwrap(), Rule::FanOut));
}

/// Salary from a known employer spent on payday is not pass-through layering;
/// the same flow from a stranger is.
#[test]
fn pass_through_ignores_known_payers() {
    let cfg = only(Rule::PassThrough);
    let mut e = Engine::new(cfg.clone());
    e.process(&tx("m1", 0, 300_000, "employer", "alice")); // establishes employer
    e.process(&tx("m2", 30 * DAY, 300_000, "employer", "alice"));
    let v = e.process(&tx(
        "spend",
        30 * DAY + 10 * MIN,
        290_000,
        "alice",
        "landlord",
    ));
    assert!(!has(&v, Rule::PassThrough), "{v:#?}");

    let mut e = Engine::new(cfg);
    e.process(&tx("in", 0, 300_000, "stranger", "mule"));
    let v = e.process(&tx("out", 10 * MIN, 290_000, "mule", "exit"));
    assert!(has(&v, Rule::PassThrough));
}

/// A->B->A repayment between established friends scores below review on its own;
/// a fresh near-full round trip still reaches review.
#[test]
fn two_hop_reciprocity_damped_unless_fresh_round_trip() {
    let cfg = only(Rule::Cycle);
    let mut e = Engine::new(cfg.clone());
    e.process(&tx("old", 0, 5_000, "bob", "ann")); // bob has paid ann before
    e.process(&tx("lend", 10 * DAY, 50_000, "ann", "bob"));
    let v = e.process(&tx("repay", 11 * DAY, 30_000, "bob", "ann"));
    assert!(v.score < cfg.bands.review, "{v:#?}");

    let mut e = Engine::new(cfg.clone());
    let v = ring(&mut e, 2);
    assert!(v.score >= cfg.bands.review, "{v:#?}");
}

/// A 3-hop loop that returns only a sliver of the value is not layering.
#[test]
fn low_retention_cycle_stays_below_review() {
    let cfg = only(Rule::Cycle);
    let mut e = Engine::new(cfg.clone());
    e.process(&tx("l1", 0, 1_000_000, "a", "b"));
    e.process(&tx("l2", HOUR, 900_000, "b", "c"));
    let v = e.process(&tx("l3", 2 * HOUR, 60_000, "c", "a"));
    assert!(has(&v, Rule::Cycle), "still reported as context");
    assert!(v.score < cfg.bands.review, "{v:#?}");
}
