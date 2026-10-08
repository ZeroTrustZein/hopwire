//! Deterministic AML detectors. Each returns `Option<Finding>`; the finding's
//! `path` is the auditable explanation.
//!
//! All window scans walk newest-first and stop after [`SCAN_CAP`] edges, so cost per
//! transaction is bounded even for hub accounts. Sums/counts reported for a capped scan
//! are lower bounds (stated as "at least" in the reason).

use std::collections::VecDeque;

use hashbrown::{HashMap, HashSet};

use crate::config::Config;
use crate::graph::{lower_bound, Graph, PaymentEdge, UsesEdge};
use crate::types::{EdgeKind, Finding, PathEdge, Rule, Scheme, Txn};

/// Max edges examined per window scan.
pub const SCAN_CAP: usize = 1024;
/// Max edges reported in a finding path.
const PATH_CAP: usize = 20;

/// Format minor units as major units with currency, e.g. `"1234.50 GBP"`.
pub fn fmt_amt(minor: i64, ccy: &str) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    format!("{sign}{}.{:02} {ccy}", minor.abs() / 100, minor.abs() % 100)
}

/// Compact human duration, e.g. `"45m"`, `"3h"`.
pub fn fmt_dur(ms: i64) -> String {
    let ms = ms.max(0);
    match ms {
        0..1_000 => format!("{ms}ms"),
        1_000..60_000 => format!("{}s", ms / 1_000),
        60_000..3_600_000 => format!("{}m", ms / 60_000),
        3_600_000..86_400_000 => format!("{}h", ms / 3_600_000),
        _ => format!("{}d", ms / 86_400_000),
    }
}

/// The current transaction as a payment path edge.
fn txn_edge(txn: &Txn) -> PathEdge {
    PathEdge {
        kind: EdgeKind::Payment,
        from: txn.debtor.clone(),
        to: txn.creditor.clone(),
        tx_id: txn.tx_id.clone(),
        ts_ms: txn.ts_ms,
        amount_minor: txn.amount_minor,
    }
}

/// Payment edges of `dq` within `[from_ts, txn.ts]`, newest first, at most `SCAN_CAP`.
fn window(
    dq: &VecDeque<PaymentEdge>,
    from_ts: i64,
    to_ts: i64,
) -> impl Iterator<Item = &PaymentEdge> {
    dq.range(lower_bound(dq, from_ts)..)
        .rev()
        .filter(move |e| e.ts <= to_ts)
        .take(SCAN_CAP)
}

fn at_least(capped: bool) -> &'static str {
    if capped {
        "at least "
    } else {
        ""
    }
}

/// `score` that ramps from 0.5 at `n == min` to `hi` at `n >= 2 * min`.
fn ramp(n: u32, min: u32, hi: f64) -> f64 {
    let x = (n.saturating_sub(min) as f64 / min.max(1) as f64).min(1.0);
    0.5 + (hi - 0.5) * x
}

// ---------------------------------------------------------------------------
// Cycle
// ---------------------------------------------------------------------------

/// Payment cycle closed by `txn` (debtor `u` -> creditor `v`): a time-respecting path
/// `v -> ... -> u` of 1..max_hops-1 existing edges, each edge no earlier than the previous,
/// all within `[txn.ts - max_span_ms, txn.ts]`, with no hub as an intermediate node
/// (`u`/`v` themselves may be hubs).
///
/// Meet in the middle: one backward level collects `u`'s in-edges (keeping the latest per
/// sender), then forward BFS from `v` to depth `max_hops - 2` checks each reached node
/// against it. Every edge examined counts against `cycle.budget`; exceeding it gives up.
/// BFS order makes the first (shortest) cycle found deterministic.
pub fn cycle(g: &Graph, u: u32, v: u32, txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.cycle;
    if !c.enabled || c.max_hops < 2 || u == v {
        return None;
    }
    let earliest = txn.ts_ms - c.max_span_ms;
    let mut visits = 0u32;
    let mut spend = |n: usize| {
        visits += n as u32;
        visits <= c.budget
    };

    // Backward level: latest edge x -> u per sender x, within the span. A hub `u` keeps
    // only its newest in-edges (half the budget) so the forward search still gets to run.
    let inc = &g.node(u).inc;
    let hi = lower_bound(inc, txn.ts_ms + 1);
    let lo = lower_bound(inc, earliest).max(hi.saturating_sub(c.budget as usize / 2));
    if lo == hi {
        return None;
    }
    spend(hi - lo);
    let mut into_u: HashMap<u32, PaymentEdge> = HashMap::with_capacity(hi - lo);
    for e in inc.range(lo..hi) {
        into_u.insert(e.peer, *e);
    }
    // `x` closes the cycle if some x -> u edge is no earlier than our arrival at x.
    let closing = |x: u32, arrived: i64| into_u.get(&x).filter(|e| e.ts >= arrived).copied();

    // Forward BFS arena: (node, parent index, edge into node). Root = v.
    let mut arena: Vec<(u32, usize, PaymentEdge)> = vec![(
        v,
        usize::MAX,
        PaymentEdge {
            peer: v,
            tx: 0,
            ts: earliest,
            amount: 0,
        },
    )];
    if let Some(last) = closing(v, earliest) {
        return Some(cycle_finding(g, &arena, 0, u, last, txn, cfg));
    }
    let max_depth = c.max_hops as usize - 2;
    let mut level = 0..1;
    for _ in 0..max_depth {
        let next_start = arena.len();
        for i in level {
            let (cur, _, arrived) = arena[i];
            let out = &g.node(cur).out;
            for e in out.range(lower_bound(out, arrived.ts)..) {
                if e.ts > txn.ts_ms {
                    break;
                }
                if !spend(1) {
                    return None;
                }
                let x = e.peer;
                if x == u || on_path(&arena, i, x) || g.is_hub(x) {
                    continue;
                }
                arena.push((x, i, *e));
                if let Some(last) = closing(x, e.ts) {
                    let leaf = arena.len() - 1;
                    return Some(cycle_finding(g, &arena, leaf, u, last, txn, cfg));
                }
            }
        }
        level = next_start..arena.len();
    }
    None
}

/// Whether `x` is already on the arena path ending at `i`.
fn on_path(arena: &[(u32, usize, PaymentEdge)], mut i: usize, x: u32) -> bool {
    while i != usize::MAX {
        if arena[i].0 == x {
            return true;
        }
        i = arena[i].1;
    }
    false
}

fn cycle_finding(
    g: &Graph,
    arena: &[(u32, usize, PaymentEdge)],
    leaf: usize,
    u: u32,
    last: PaymentEdge,
    txn: &Txn,
    cfg: &Config,
) -> Finding {
    // Unwind v -> ... -> x, then x -> u (`last` is from u's in-deque, so `peer` is x).
    let mut edges = Vec::new();
    let mut i = leaf;
    while i != 0 {
        let (_, parent, e) = arena[i];
        edges.push((arena[parent].0, e));
        i = parent;
    }
    edges.reverse();
    edges.push((last.peer, PaymentEdge { peer: u, ..last }));
    cycle_finding_from(g, &edges, txn, cfg)
}

fn cycle_finding_from(g: &Graph, edges: &[(u32, PaymentEdge)], txn: &Txn, cfg: &Config) -> Finding {
    let hops = edges.len() + 1;
    let span = txn.ts_ms - edges[0].1.ts;
    let amounts = edges
        .iter()
        .map(|(_, e)| e.amount)
        .chain([txn.amount_minor]);
    let (lo, hi) = amounts.fold((i64::MAX, i64::MIN), |(lo, hi), a| (lo.min(a), hi.max(a)));
    let retention = lo as f64 / hi.max(1) as f64;
    let speed = 1.0 - (span as f64 / cfg.cycle.max_span_ms.max(1) as f64).clamp(0.0, 1.0);
    let score = 0.5 + 0.3 * retention + 0.2 * speed;
    let mut path: Vec<PathEdge> = edges.iter().map(|(f, e)| g.pay_edge(*f, e)).collect();
    path.push(txn_edge(txn));
    let reason = format!(
        "Funds returned to {} through a {hops}-hop cycle within {}; {} closed the loop and {:.0}% of the largest leg was retained.",
        txn.creditor,
        fmt_dur(span),
        fmt_amt(txn.amount_minor, &txn.currency),
        retention * 100.0
    );
    Finding {
        rule: Rule::Cycle,
        score,
        reason,
        path,
    }
}

// ---------------------------------------------------------------------------
// MuleFanIn
// ---------------------------------------------------------------------------

/// Creditor received from >= `min_senders` distinct senders within `fan_in.window_ms`
/// (this txn included). A finding requires either a young account or a previously unseen
/// sender; this suppresses established merchant/card collection accounts.
pub fn fan_in(g: &Graph, u: u32, v: u32, txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.fan_in;
    // Card merchant aggregation is expected high fan-in; reserve this rule for
    // account-to-account rails where a collection pattern is meaningful.
    if !c.enabled || txn.scheme == Scheme::Card || g.node(v).inc.len() + 1 < c.min_senders as usize
    {
        return None;
    }
    let edges: Vec<_> = window(&g.node(v).inc, txn.ts_ms - c.window_ms, txn.ts_ms).collect();
    let senders: HashSet<u32> = edges.iter().map(|e| e.peer).chain([u]).collect();
    let n = senders.len() as u32;
    if n < c.min_senders {
        return None;
    }
    let opened = txn.creditor_account_opened_at_ms.or(g.node(v).opened_at_ms);
    let age = opened
        .map(|o| txn.ts_ms - o)
        .filter(|a| (0..=c.young_account_ms).contains(a));
    let new_senders = edges
        .iter()
        .filter(|e| !g.node(v).known_payers.has_seen(e.peer))
        .count() as u32
        + u32::from(!g.node(v).known_payers.has_seen(u));
    if age.is_none() && new_senders < c.min_new_senders {
        return None;
    }
    let total: i64 = edges.iter().map(|e| e.amount).sum::<i64>() + txn.amount_minor;
    let score = (ramp(n, c.min_senders, 0.85) + if age.is_some() { 0.15 } else { 0.0 }).min(1.0);
    let mut path = vec![txn_edge(txn)];
    path.extend(edges.iter().take(PATH_CAP - 1).map(|e| g.pay_edge_in(v, e)));
    let young = age.map_or(String::new(), |a| {
        format!(" The account was opened only {} earlier.", fmt_dur(a))
    });
    let reason = format!(
        "{} received {} from {}{n} distinct senders within {}.{young}",
        txn.creditor,
        fmt_amt(total, &txn.currency),
        at_least(edges.len() == SCAN_CAP),
        fmt_dur(c.window_ms)
    );
    Some(Finding {
        rule: Rule::MuleFanIn,
        score,
        reason,
        path,
    })
}

// ---------------------------------------------------------------------------
// PassThrough
// ---------------------------------------------------------------------------

/// The debtor is forwarding money it just received:
/// - inflow = sum of payments into the debtor during `[t - within_ms, t]`; must be >= `min_inflow_minor`;
/// - outflow = this txn + debtor's other payments sent at or after the earliest such inflow;
/// - fires when `outflow >= min_ratio * inflow`.
///
/// Path = the inflow edges (newest first, capped) followed by this outflow.
pub fn pass_through(g: &Graph, u: u32, _v: u32, txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.pass_through;
    if !c.enabled {
        return None;
    }
    let node = g.node(u);
    let ins: Vec<_> = window(&node.inc, txn.ts_ms - c.within_ms, txn.ts_ms).collect();
    let inflow: i64 = ins.iter().map(|e| e.amount).sum();
    if inflow < c.min_inflow_minor.max(1) {
        return None;
    }
    let first_in = ins.last()?.ts;
    // Attribute only outgoing payments after actual inflow; unrelated earlier outflow
    // in the lookback window must not inflate pass-through ratio.
    let outflow = txn.amount_minor
        + window(&node.out, first_in, txn.ts_ms)
            .filter(|e| e.ts >= first_in)
            .map(|e| e.amount)
            .sum::<i64>();
    let ratio = outflow as f64 / inflow as f64;
    if ratio < c.min_ratio {
        return None;
    }
    let delay = txn.ts_ms - ins[0].ts;
    let speed = 1.0 - (delay as f64 / c.within_ms.max(1) as f64).clamp(0.0, 1.0);
    let fullness = ((ratio - c.min_ratio) / (1.0 - c.min_ratio).max(1e-6)).clamp(0.0, 1.0);
    let score = 0.6 + 0.25 * fullness + 0.15 * speed;
    let mut path: Vec<_> = ins
        .iter()
        .take(PATH_CAP - 1)
        .map(|e| g.pay_edge_in(u, e))
        .collect();
    path.push(txn_edge(txn));
    let reason = format!(
        "{} forwarded {} ({:.0}% of {} received from {} payment(s)) within {} of receiving it.",
        txn.debtor,
        fmt_amt(outflow, &txn.currency),
        ratio * 100.0,
        fmt_amt(inflow, &txn.currency),
        ins.len(),
        fmt_dur(txn.ts_ms - first_in)
    );
    Some(Finding {
        rule: Rule::PassThrough,
        score,
        reason,
        path,
    })
}

// ---------------------------------------------------------------------------
// FanOut
// ---------------------------------------------------------------------------

/// Debtor paid >= `min_payees` distinct *new* payees within `fan_out.window_ms`.
/// Payees opened within `young_payee_ms` boost the score by 0.15, allowing smaller
/// newly created scatter rings to alert.
pub fn fan_out(g: &Graph, u: u32, v: u32, txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.fan_out;
    if !c.enabled || g.node(u).out.len() + 1 < c.min_payees as usize {
        return None;
    }
    let edges: Vec<_> = window(&g.node(u).out, txn.ts_ms - c.window_ms, txn.ts_ms).collect();
    let known = &g.node(u).known_payees;
    // Lifetime novelty suppresses recurring payroll after first run; young payees
    // affect score only, never eligibility, so mixed scatter rings remain detectable.
    let mut new_payees: HashSet<u32> = edges
        .iter()
        .filter(|e| known.is_new(e.peer, txn.ts_ms - c.window_ms))
        .map(|e| e.peer)
        .collect();
    if known.is_new(v, txn.ts_ms - c.window_ms) {
        new_payees.insert(v);
    }
    let n = new_payees.len() as u32;
    let young_precheck = new_payees
        .iter()
        .filter(|&&p| {
            g.node(p)
                .opened_at_ms
                .is_some_and(|o| (0..=c.young_payee_ms).contains(&(txn.ts_ms - o)))
        })
        .count();
    // Young/new recipient clusters are stronger than the generic threshold. This
    // catches compact laundering scatters without reopening recurring payroll.
    if n < c.min_payees && young_precheck < 1 {
        return None;
    }
    let total: i64 = edges
        .iter()
        .filter(|e| new_payees.contains(&e.peer))
        .map(|e| e.amount)
        .sum::<i64>()
        + if new_payees.contains(&v) {
            txn.amount_minor
        } else {
            0
        };
    let mut path = Vec::new();
    if new_payees.contains(&v) {
        path.push(txn_edge(txn));
    }
    path.extend(
        edges
            .iter()
            .filter(|e| new_payees.contains(&e.peer))
            .take(PATH_CAP - path.len())
            .map(|e| g.pay_edge(u, e)),
    );
    let young_count = new_payees
        .iter()
        .filter(|&&p| {
            g.node(p)
                .opened_at_ms
                .is_some_and(|o| (0..=c.young_payee_ms).contains(&(txn.ts_ms - o)))
        })
        .count();
    let never_seen_count = new_payees.iter().filter(|&&p| !known.has_seen(p)).count();
    let score = (ramp(n, c.min_payees, 0.95)
        + if young_count > 0 || never_seen_count >= c.min_payees as usize {
            0.45
        } else {
            -0.15
        })
    .clamp(0.0, 1.0);
    let reason = format!(
        "{} paid {} to {}{n} new payees ({} recently opened) within {}.",
        txn.debtor,
        fmt_amt(total, &txn.currency),
        at_least(edges.len() == SCAN_CAP),
        young_count,
        fmt_dur(c.window_ms)
    );
    Some(Finding {
        rule: Rule::FanOut,
        score,
        reason,
        path,
    })
}

// ---------------------------------------------------------------------------
// Velocity
// ---------------------------------------------------------------------------

/// Debtor's outgoing count of new counterparties > `max_count` or new-payee sum >
/// `max_sum_minor` within `velocity.window_ms` (this txn included).
pub fn velocity(g: &Graph, u: u32, v: u32, txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.velocity;
    if !c.enabled {
        return None;
    }
    let edges: Vec<_> = window(&g.node(u).out, txn.ts_ms - c.window_ms, txn.ts_ms).collect();
    let known = &g.node(u).known_payees;
    let new_edges: Vec<_> = edges
        .iter()
        .filter(|e| known.is_new(e.peer, txn.ts_ms - c.window_ms))
        .collect();
    let new_current = known.is_new(v, txn.ts_ms - c.window_ms);
    let new_count = new_edges.len() as u32 + u32::from(new_current);
    let count = edges.len() as u32 + 1;
    let new_sum: i64 = new_edges.iter().map(|e| e.amount).sum::<i64>()
        + if new_current { txn.amount_minor } else { 0 };
    if new_count == 0 {
        return None;
    }
    if new_count <= c.max_count && new_sum <= c.max_sum_minor {
        return None;
    }
    let over = |x: f64, max: f64| (x / max.max(1.0) - 1.0).max(0.0);
    let severity = over(new_count as f64, c.max_count as f64)
        .max(over(new_sum as f64, c.max_sum_minor as f64));
    let score = 0.5 + 0.45 * severity.min(1.0);
    let mut path = Vec::new();
    if new_current {
        path.push(txn_edge(txn));
    }
    path.extend(
        new_edges
            .iter()
            .take(PATH_CAP - path.len())
            .map(|e| g.pay_edge(u, e)),
    );
    let reason = format!(
        "{} made {}{count} payments to new payees totalling {} within {} (limits: {} new payees or {}).",
        txn.debtor,
        at_least(edges.len() == SCAN_CAP),
        fmt_amt(new_sum, &txn.currency),
        fmt_dur(c.window_ms),
        c.max_count,
        fmt_amt(c.max_sum_minor, &txn.currency)
    );
    Some(Finding {
        rule: Rule::Velocity,
        score,
        reason,
        path,
    })
}

// ---------------------------------------------------------------------------
// SharedIdentifier
// ---------------------------------------------------------------------------

/// An identifier (device / ip / customer) touched by this txn is linked, within the graph
/// window, to >= `min_accounts` distinct accounts (including this txn's account).
/// `links` = (account, identifier) pairs from this txn. Strongest identifier wins.
pub fn shared_id(g: &Graph, links: &[(u32, u32)], txn: &Txn, cfg: &Config) -> Option<Finding> {
    let c = &cfg.shared_id;
    if !c.enabled {
        return None;
    }
    let mut best: Option<(u32, u32, u32, Vec<&UsesEdge>)> = None;
    for &(acct, ident) in links {
        let uses = &g.node(ident).uses_in;
        let edges: Vec<_> = uses.iter().rev().take(SCAN_CAP).collect();
        let n = edges
            .iter()
            .map(|e| e.peer)
            .chain([acct])
            .collect::<HashSet<_>>()
            .len() as u32;
        if n >= c.min_accounts && best.as_ref().is_none_or(|b| n > b.2) {
            best = Some((acct, ident, n, edges));
        }
    }
    let (acct, ident, n, edges) = best?;
    let id = &g.node(ident).id;
    let mut path = vec![PathEdge {
        kind: EdgeKind::Uses,
        from: g.node(acct).id.to_string(),
        to: id.to_string(),
        tx_id: txn.tx_id.clone(),
        ts_ms: txn.ts_ms,
        amount_minor: 0,
    }];
    path.extend(
        edges
            .iter()
            .filter(|e| e.peer != acct)
            .take(PATH_CAP - 1)
            .map(|e| g.uses_edge(ident, e)),
    );
    let reason = format!(
        "Identifier {id} is shared by {}{n} distinct accounts within the last {}.",
        at_least(edges.len() == SCAN_CAP),
        fmt_dur(cfg.window_ms)
    );
    Some(Finding {
        rule: Rule::SharedIdentifier,
        score: ramp(n, c.min_accounts, 0.95),
        reason,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(fmt_amt(123_450, "GBP"), "1234.50 GBP");
        assert_eq!(fmt_amt(5, "EUR"), "0.05 EUR");
        assert_eq!(fmt_dur(45_000), "45s");
        assert_eq!(fmt_dur(2 * 3_600_000), "2h");
        assert_eq!(fmt_dur(3 * 86_400_000), "3d");
    }

    #[test]
    fn ramp_bounds() {
        assert_eq!(ramp(4, 4, 0.9), 0.5);
        assert!((ramp(8, 4, 0.9) - 0.9).abs() < 1e-9);
        assert!((ramp(100, 4, 0.9) - 0.9).abs() < 1e-9);
    }
}
