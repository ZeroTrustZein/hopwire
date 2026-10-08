//! Common benchmark runner: feeds txns (ts order) to an in-process `aegis_core::Engine`,
//! records per-txn `process()` latency, scores detection against labels, writes the report.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use aegis_core::{Config, Decision, Engine, Rule, Txn};
use hdrhistogram::Histogram;

use crate::{Label, Res};

#[derive(Default)]
struct Typo {
    txns: u64,
    flagged: u64,
    attempts: BTreeSet<u64>,
    caught: BTreeSet<u64>,
    /// Txns of this typology carrying a finding of each rule (flagged or not).
    rules: BTreeMap<Rule, u64>,
}

pub fn run(
    cfg: Config,
    dataset: &str,
    items: impl Iterator<Item = (Txn, Option<Label>)>,
    report: &Path,
) -> Res<()> {
    let mut eng = Engine::new(cfg.clone());
    let mut hist = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3)?; // ns
    let (mut n, mut flagged, mut tp, mut laundering, mut review, mut block) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut engine_ns = 0u128;
    // rule -> (txns with finding, of which laundering, of which flagged)
    let mut rule_hits: BTreeMap<Rule, (u64, u64, u64)> = BTreeMap::new();
    let mut typos: BTreeMap<String, Typo> = BTreeMap::new();
    let (mut first_ts, mut last_ts) = (i64::MAX, i64::MIN);

    eprintln!("bench: {dataset}");
    let wall = Instant::now();
    for (txn, label) in items {
        let t = Instant::now();
        let v = eng.process(&txn);
        let ns = t.elapsed().as_nanos() as u64;
        hist.saturating_record(ns.max(1));
        engine_ns += ns as u128;
        n += 1;
        first_ts = first_ts.min(txn.ts_ms);
        last_ts = last_ts.max(txn.ts_ms);

        let hit = v.decision != Decision::Allow;
        match v.decision {
            Decision::Review => review += 1,
            Decision::Block => block += 1,
            Decision::Allow => {}
        }
        flagged += u64::from(hit);
        let mut rules: Vec<Rule> = v.findings.iter().map(|f| f.rule).collect();
        rules.sort();
        rules.dedup();
        for r in &rules {
            let e = rule_hits.entry(*r).or_default();
            e.0 += 1;
            e.1 += u64::from(label.is_some());
            e.2 += u64::from(hit);
        }
        if let Some(l) = label {
            laundering += 1;
            tp += u64::from(hit);
            let s = typos.entry(l.typology).or_default();
            s.txns += 1;
            s.flagged += u64::from(hit);
            if let Some(a) = l.attempt {
                s.attempts.insert(a);
                if hit {
                    s.caught.insert(a);
                }
            }
            for r in rules {
                *s.rules.entry(r).or_default() += 1;
            }
        }
        if n % 1_000_000 == 0 {
            eprintln!(
                "bench: {n} txns, {flagged} flagged, {:.1}s",
                wall.elapsed().as_secs_f64()
            );
        }
    }
    let wall_s = wall.elapsed().as_secs_f64();
    let stats = eng.stats();

    let pct = |a: u64, b: u64| {
        if b == 0 {
            "-".to_string()
        } else {
            format!("{:.1}%", 100.0 * a as f64 / b as f64)
        }
    };
    let us = |ns: u64| format!("{:.1}", ns as f64 / 1000.0);
    let q = |p: f64| us(hist.value_at_quantile(p));
    let engine_tps = n as f64 / (engine_ns as f64 / 1e9).max(1e-9);
    let span_days = if n == 0 {
        0.0
    } else {
        (last_ts - first_ts) as f64 / 86_400_000.0
    };

    let mut md = String::new();
    let w = &mut md;
    writeln!(w, "# AegisGraph benchmark report\n")?;
    writeln!(w, "| | |\n|---|---|")?;
    writeln!(w, "| Date | {} |", today_utc())?;
    writeln!(w, "| Dataset | {dataset} |")?;
    writeln!(w, "| Transactions | {n} over {span_days:.1} days |")?;
    writeln!(
        w,
        "| Engine | aegis-core {} (in-process, single thread) |",
        aegis_core::version()
    )?;
    for (k, v) in machine() {
        writeln!(w, "| {k} | {v} |")?;
    }
    writeln!(w, "\n## Throughput and latency\n")?;
    writeln!(
        w,
        "Latency = wall time of one `Engine::process()` call (ingest + all detectors).\n"
    )?;
    writeln!(w, "| Engine TPS | Wall TPS (incl. data gen/parse) | p50 µs | p90 µs | p99 µs | p99.9 µs | max µs | mean µs |")?;
    writeln!(w, "|---|---|---|---|---|---|---|---|")?;
    writeln!(
        w,
        "| {engine_tps:.0} | {:.0} | {} | {} | {} | {} | {} | {:.1} |",
        n as f64 / wall_s,
        q(0.5),
        q(0.9),
        q(0.99),
        q(0.999),
        us(hist.max()),
        hist.mean() / 1000.0
    )?;
    writeln!(
        w,
        "\nFinal graph: {} nodes, {} edges, {} hubs (window {} h).",
        stats.nodes,
        stats.edges,
        stats.hubs,
        cfg.window_ms / 3_600_000
    )?;

    writeln!(w, "\n## Detection\n")?;
    writeln!(w, "Flagged = decision `review` or `block`.\n")?;
    writeln!(w, "| Laundering txns | Flagged | Review | Block | Alert rate | Precision (flagged laundering / flagged) | Txn recall |")?;
    writeln!(w, "|---|---|---|---|---|---|---|")?;
    writeln!(
        w,
        "| {laundering} | {flagged} | {review} | {block} | {} | {} | {} |",
        pct(flagged, n),
        pct(tp, flagged),
        pct(tp, laundering)
    )?;

    writeln!(w, "\n### Recall by typology\n")?;
    writeln!(w, "Attempt recall: an attempt counts as caught if any of its txns is flagged. Rules = findings on that typology's txns (flagged or not).\n")?;
    writeln!(
        w,
        "| Typology | Attempts | Attempt recall | Txns | Txn recall | Rules fired |"
    )?;
    writeln!(w, "|---|---|---|---|---|---|")?;
    for (name, s) in &typos {
        let rules: Vec<String> = s
            .rules
            .iter()
            .map(|(r, c)| format!("{}:{c}", rule_name(*r)))
            .collect();
        writeln!(
            w,
            "| {name} | {} | {} | {} | {} | {} |",
            s.attempts.len(),
            pct(s.caught.len() as u64, s.attempts.len() as u64),
            s.txns,
            pct(s.flagged, s.txns),
            if rules.is_empty() {
                "-".into()
            } else {
                rules.join(", ")
            }
        )?;
    }

    writeln!(w, "\n### Rule hits\n")?;
    writeln!(
        w,
        "| Rule | Txns with finding | On laundering txns | On flagged txns |"
    )?;
    writeln!(w, "|---|---|---|---|")?;
    for (r, (a, b, c)) in &rule_hits {
        writeln!(w, "| {} | {a} | {b} | {c} |", rule_name(*r))?;
    }

    writeln!(w, "\n## Config\n\n```toml\n{}```", toml::to_string(&cfg)?)?;

    if let Some(dir) = report.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(report, &md)?;
    println!("{md}");
    eprintln!("bench: report written to {}", report.display());
    Ok(())
}

fn rule_name(r: Rule) -> String {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn machine() -> Vec<(&'static str, String)> {
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let cpu = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    vec![
        ("CPU", format!("{cpu} ({} logical cores)", sys.cpus().len())),
        (
            "RAM",
            format!("{:.1} GiB", sys.total_memory() as f64 / (1u64 << 30) as f64),
        ),
        ("OS", sysinfo::System::long_os_version().unwrap_or_default()),
        ("rustc", rustc.trim().to_string()),
        (
            "Build",
            if cfg!(debug_assertions) {
                "debug".into()
            } else {
                "release".into()
            },
        ),
    ]
}

/// Today's UTC date as YYYY-MM-DD (Hinnant's civil_from_days).
fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}
