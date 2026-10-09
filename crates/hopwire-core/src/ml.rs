//! Embedded Rust ONNX Runtime for Temporal Graph Network (TGN) inference.
//!
//! Uses `tract-onnx` for pure-Rust, zero-external-dependency model execution.
//! Maintains dynamic node memory vectors and continuous-time updates in memory.

#[cfg(feature = "onnx")]
pub mod tgn {
    use std::path::Path;
    use std::sync::Arc;

    use crate::{EdgeKind, Finding, MlScorer, PathEdge, Rule, Scheme, Txn};
    use hashbrown::HashMap;
    use tract_onnx::prelude::*;

    pub struct TgnScorer {
        model: Arc<TypedRunnableModel>,
        threshold: f32,
        #[allow(dead_code)]
        weight: f64,
        /// In-memory node states: node_id -> 32-dim memory vector
        memories: HashMap<String, [f32; 32]>,
        last_update: HashMap<String, i64>,
    }

    impl TgnScorer {
        /// Load ONNX model from file path (e.g., `ml/models/tgn_scoring.onnx`).
        pub fn load(path: impl AsRef<Path>, threshold: f32, weight: f64) -> TractResult<Self> {
            let model = tract_onnx::onnx()
                .model_for_path(path)?
                .into_optimized()?
                .into_runnable()?;

            Ok(Self {
                model,
                threshold,
                weight,
                memories: HashMap::new(),
                last_update: HashMap::new(),
            })
        }

        pub fn memory_len(&self) -> usize {
            self.memories.len()
        }
    }

    impl MlScorer for TgnScorer {
        fn score(&mut self, txn: &Txn) -> Option<Finding> {
            let src_mem = self.memories.get(&txn.debtor).copied().unwrap_or([0.0; 32]);
            let dst_mem = self
                .memories
                .get(&txn.creditor)
                .copied()
                .unwrap_or([0.0; 32]);

            let last_src = self
                .last_update
                .get(&txn.debtor)
                .copied()
                .unwrap_or(txn.ts_ms);
            let last_dst = self
                .last_update
                .get(&txn.creditor)
                .copied()
                .unwrap_or(txn.ts_ms);
            let min_last = last_src.min(last_dst);
            let dt_secs = ((txn.ts_ms - min_last).max(0) as f32) / 1000.0;

            let log_amt = (txn.amount_minor as f32 / 100.0).ln_1p();
            let mut edge_feat = [0.0f32; 8];
            edge_feat[0] = log_amt;

            // One-hot scheme mapping (approximate format feature)
            match txn.scheme {
                Scheme::Fps | Scheme::Bacs => edge_feat[1] = 1.0, // ACH
                Scheme::Chaps => edge_feat[2] = 1.0,              // Wire
                Scheme::Card => edge_feat[3] = 1.0,               // Credit Card
                _ => edge_feat[4] = 1.0,
            }

            // Build Tract tensors
            let t_src = tract_ndarray::Array2::from_shape_vec((1, 32), src_mem.to_vec())
                .ok()?
                .into_tensor();
            let t_dst = tract_ndarray::Array2::from_shape_vec((1, 32), dst_mem.to_vec())
                .ok()?
                .into_tensor();
            let t_dt = tract_ndarray::Array2::from_shape_vec((1, 1), vec![dt_secs])
                .ok()?
                .into_tensor();
            let t_feat = tract_ndarray::Array2::from_shape_vec((1, 8), edge_feat.to_vec())
                .ok()?
                .into_tensor();

            let outputs = self
                .model
                .run(tvec![
                    t_src.into(),
                    t_dst.into(),
                    t_dt.into(),
                    t_feat.into()
                ])
                .ok()?;

            let risk_score = outputs[0].to_plain_array_view::<f32>().ok()?[[0, 0]];

            // Update memories from model outputs
            if let Ok(new_src_slice) = outputs[1].to_plain_array_view::<f32>() {
                let mut v = [0.0f32; 32];
                for (i, val) in new_src_slice.iter().take(32).enumerate() {
                    v[i] = *val;
                }
                self.memories.insert(txn.debtor.clone(), v);
            }
            if let Ok(new_dst_slice) = outputs[2].to_plain_array_view::<f32>() {
                let mut v = [0.0f32; 32];
                for (i, val) in new_dst_slice.iter().take(32).enumerate() {
                    v[i] = *val;
                }
                self.memories.insert(txn.creditor.clone(), v);
            }

            self.last_update.insert(txn.debtor.clone(), txn.ts_ms);
            self.last_update.insert(txn.creditor.clone(), txn.ts_ms);

            if risk_score >= self.threshold {
                Some(Finding {
                    rule: Rule::Ml,
                    score: (risk_score as f64).clamp(0.0, 1.0),
                    reason: format!(
                        "Continuous-Time TGN neural network flagged structural anomaly ({:.1}% anomaly score, delta-t {:.1}s).",
                        risk_score * 100.0,
                        dt_secs
                    ),
                    path: vec![PathEdge {
                        kind: EdgeKind::Payment,
                        from: txn.debtor.clone(),
                        to: txn.creditor.clone(),
                        tx_id: txn.tx_id.clone(),
                        ts_ms: txn.ts_ms,
                        amount_minor: txn.amount_minor,
                    }],
                })
            } else {
                None
            }
        }
    }
}
