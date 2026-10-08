//! Engine public surface. Signatures are FROZEN (server/bench code against them).
//! Bodies are stubs until Phase 1 lands.

use crate::{Config, EngineStats, Finding, Subgraph, Txn, Verdict};

/// Extension seam for the ML path (TGN via ONNX). Called after deterministic
/// detectors; may return an extra `Rule::Ml` finding.
pub trait MlScorer: Send {
    fn score(&mut self, txn: &Txn) -> Option<Finding>;
}

pub struct Engine {
    cfg: Config,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self { cfg }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Ingest `txn` into the graph and score it. Main hot path.
    /// Timestamps are expected roughly non-decreasing; late events are accepted,
    /// eviction is driven by the max timestamp seen (watermark).
    pub fn process(&mut self, txn: &Txn) -> Verdict {
        let _ = txn;
        todo!("phase 1")
    }

    /// Ingest without scoring (used to rebuild state on restart).
    pub fn ingest(&mut self, txn: &Txn) {
        let _ = txn;
        todo!("phase 1")
    }

    /// Neighbourhood of `node_id` (external ID, see `NodeKind` docs) up to `hops`,
    /// at most `max_edges` edges. Unknown node -> empty subgraph.
    pub fn subgraph(&self, node_id: &str, hops: u8, max_edges: usize) -> Subgraph {
        let _ = (node_id, hops, max_edges);
        todo!("phase 1")
    }

    pub fn stats(&self) -> EngineStats {
        todo!("phase 1")
    }

    pub fn set_ml_scorer(&mut self, scorer: Box<dyn MlScorer>) {
        let _ = scorer;
        todo!("phase 1")
    }
}
