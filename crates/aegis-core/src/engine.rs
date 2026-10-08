//! Engine public surface. Signatures are FROZEN (server/bench code against them).

use std::time::Instant;

use crate::graph::Graph;
use crate::{detectors as d, score};
use crate::{Config, EngineStats, Finding, NodeKind, Subgraph, Txn, Verdict};

/// Nodes visited by the background round-robin eviction per transaction.
const SWEEP_PER_TXN: usize = 8;

/// Extension seam for the ML path (TGN via ONNX). Called after deterministic
/// detectors; may return an extra `Rule::Ml` finding. Its score enters the
/// noisy-OR with weight 1.0 (the scorer owns its own calibration).
pub trait MlScorer: Send {
    fn score(&mut self, txn: &Txn) -> Option<Finding>;
}

/// Single-threaded streaming scorer over a sliding-window transaction graph.
pub struct Engine {
    cfg: Config,
    graph: Graph,
    processed: u64,
    ml: Option<Box<dyn MlScorer>>,
}

/// Node indices touched by one transaction.
struct Touched {
    debtor: u32,
    creditor: u32,
    /// (account, identifier) `Uses` links.
    links: Vec<(u32, u32)>,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        let graph = Graph::new(cfg.window_ms, cfg.hub_degree);
        Self {
            cfg,
            graph,
            processed: 0,
            ml: None,
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Ingest `txn` into the graph and score it. Main hot path.
    /// Timestamps are expected roughly non-decreasing; late events are accepted,
    /// eviction is driven by the max timestamp seen (watermark).
    /// Detectors see the graph *before* `txn` is added, plus `txn` itself.
    pub fn process(&mut self, txn: &Txn) -> Verdict {
        let start = Instant::now();
        if txn.debtor == txn.creditor {
            // Self-transfer (own pots, sub-accounts, reinvestment): no counterparty, no
            // flow through the graph. Recorded nowhere so it cannot inflate velocity or
            // fake a pass-through; the watermark still advances.
            self.graph.advance(txn.ts_ms);
            self.processed += 1;
            return Verdict {
                tx_id: txn.tx_id.clone(),
                score: 0.0,
                decision: crate::Decision::Allow,
                findings: Vec::new(),
                latency_us: start.elapsed().as_micros() as u64,
            };
        }
        let t = self.touch(txn);
        let (g, cfg) = (&self.graph, &self.cfg);
        let mut findings: Vec<Finding> = [
            d::cycle(g, t.debtor, t.creditor, txn, cfg),
            d::fan_in(g, t.debtor, t.creditor, txn, cfg),
            d::pass_through(g, t.debtor, t.creditor, txn, cfg),
            d::fan_out(g, t.debtor, t.creditor, txn, cfg),
            d::velocity(g, t.debtor, t.creditor, txn, cfg),
            d::shared_id(g, &t.links, txn, cfg),
        ]
        .into_iter()
        .flatten()
        .collect();
        findings.extend(self.ml.as_mut().and_then(|m| m.score(txn)));
        let (score, decision) = score::evaluate(&mut findings, cfg);
        self.commit(txn, &t);
        Verdict {
            tx_id: txn.tx_id.clone(),
            score,
            decision,
            findings,
            latency_us: start.elapsed().as_micros() as u64,
        }
    }

    /// Ingest without scoring (used to rebuild state on restart).
    pub fn ingest(&mut self, txn: &Txn) {
        if txn.debtor == txn.creditor {
            self.graph.advance(txn.ts_ms);
            self.processed += 1;
            return;
        }
        let t = self.touch(txn);
        self.commit(txn, &t);
    }

    /// Advance the watermark, intern the txn's nodes and evict their stale edges.
    fn touch(&mut self, txn: &Txn) -> Touched {
        let g = &mut self.graph;
        g.advance(txn.ts_ms);
        let debtor = g.intern(&txn.debtor, NodeKind::Account);
        let creditor = g.intern(&txn.creditor, NodeKind::Account);
        if let Some(o) = txn.creditor_account_opened_at_ms {
            g.node_mut(creditor).opened_at_ms = Some(o);
        }
        let idents = [
            (debtor, "device:", NodeKind::Device, &txn.device_id),
            (debtor, "ip:", NodeKind::Ip, &txn.ip),
            (
                debtor,
                "cust:",
                NodeKind::Customer,
                &txn.debtor_customer_hash,
            ),
            (
                creditor,
                "cust:",
                NodeKind::Customer,
                &txn.creditor_customer_hash,
            ),
        ];
        let links: Vec<_> = idents
            .into_iter()
            .filter_map(|(acct, prefix, kind, v)| {
                Some((acct, g.intern(&format!("{prefix}{}", v.as_ref()?), kind)))
            })
            .collect();
        for i in [debtor, creditor]
            .into_iter()
            .chain(links.iter().map(|l| l.1))
        {
            g.evict(i);
        }
        Touched {
            debtor,
            creditor,
            links,
        }
    }

    /// Add the txn's edges and run a slice of background eviction.
    fn commit(&mut self, txn: &Txn, t: &Touched) {
        let g = &mut self.graph;
        let tx = g.txs.alloc(&txn.tx_id, 2 * (1 + t.links.len() as u32));
        g.add_payment(t.debtor, t.creditor, txn.ts_ms, txn.amount_minor, tx);
        g.node_mut(t.debtor)
            .known_payees
            .record(t.creditor, txn.ts_ms);
        g.node_mut(t.creditor)
            .known_payers
            .record(t.debtor, txn.ts_ms);
        for &(acct, ident) in &t.links {
            g.add_uses(acct, ident, txn.ts_ms, tx);
        }
        g.sweep(SWEEP_PER_TXN);
        self.processed += 1;
    }

    /// Neighbourhood of `node_id` (external ID, see `NodeKind` docs) up to `hops`,
    /// at most `max_edges` edges. Unknown node -> empty subgraph.
    pub fn subgraph(&self, node_id: &str, hops: u8, max_edges: usize) -> Subgraph {
        self.graph.subgraph(node_id, hops, max_edges)
    }

    /// Counters. `hubs` is an O(nodes) scan; call it for monitoring, not per txn.
    pub fn stats(&self) -> EngineStats {
        EngineStats {
            nodes: self.graph.nodes.len() as u64,
            edges: self.graph.edges,
            hubs: self.graph.hubs(),
            processed: self.processed,
            watermark_ms: self.graph.watermark.max(0),
        }
    }

    /// Install the ML scorer (replaces any previous one).
    pub fn set_ml_scorer(&mut self, scorer: Box<dyn MlScorer>) {
        self.ml = Some(scorer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Engine>();
    }
}
