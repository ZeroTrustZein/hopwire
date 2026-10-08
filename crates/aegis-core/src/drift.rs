//! Model Risk Governance & Drift Telemetry (Federal Reserve SR 11-7).
//!
//! Implements Population Stability Index (PSI) and Characteristic Stability
//! Index (CSI) tracking across score distributions and input variables.

use serde::{Deserialize, Serialize};

pub const PSI_STABLE_THRESHOLD: f64 = 0.10;
pub const PSI_MODERATE_THRESHOLD: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftStatus {
    Green,  // PSI < 0.10
    Yellow, // 0.10 <= PSI < 0.25
    Red,    // PSI >= 0.25
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinDistribution {
    pub bin_start: f64,
    pub bin_end: f64,
    pub baseline_pct: f64,
    pub current_pct: f64,
    pub psi_contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PsiReport {
    pub metric_name: String,
    pub total_psi: f64,
    pub status: DriftStatus,
    pub baseline_count: usize,
    pub current_count: usize,
    pub bins: Vec<BinDistribution>,
}

/// Compute Population Stability Index across 10 deciles [0.0, 1.0].
pub fn calculate_psi(metric_name: &str, baseline: &[f64], current: &[f64]) -> PsiReport {
    const NUM_BINS: usize = 10;
    const EPSILON: f64 = 1e-4;

    let b_len = baseline.len().max(1) as f64;
    let c_len = current.len().max(1) as f64;

    let mut b_counts = [0usize; NUM_BINS];
    let mut c_counts = [0usize; NUM_BINS];

    let bin_idx = |v: f64| -> usize {
        let idx = (v * NUM_BINS as f64).floor() as usize;
        idx.min(NUM_BINS - 1)
    };

    for &v in baseline {
        b_counts[bin_idx(v.clamp(0.0, 1.0))] += 1;
    }
    for &v in current {
        c_counts[bin_idx(v.clamp(0.0, 1.0))] += 1;
    }

    let mut total_psi = 0.0;
    let mut bins = Vec::with_capacity(NUM_BINS);

    for i in 0..NUM_BINS {
        let bin_start = i as f64 / NUM_BINS as f64;
        let bin_end = (i + 1) as f64 / NUM_BINS as f64;

        let b_pct = (b_counts[i] as f64 / b_len).max(EPSILON);
        let c_pct = (c_counts[i] as f64 / c_len).max(EPSILON);

        let contrib = (c_pct - b_pct) * (c_pct / b_pct).ln();
        total_psi += contrib;

        bins.push(BinDistribution {
            bin_start,
            bin_end,
            baseline_pct: b_pct,
            current_pct: c_pct,
            psi_contribution: contrib,
        });
    }

    let status = if total_psi < PSI_STABLE_THRESHOLD {
        DriftStatus::Green
    } else if total_psi < PSI_MODERATE_THRESHOLD {
        DriftStatus::Yellow
    } else {
        DriftStatus::Red
    };

    PsiReport {
        metric_name: metric_name.to_string(),
        total_psi,
        status,
        baseline_count: baseline.len(),
        current_count: current.len(),
        bins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psi_identical_distributions_is_zero() {
        let baseline = vec![0.05, 0.15, 0.25, 0.35, 0.45, 0.55, 0.65, 0.75, 0.85, 0.95];
        let current = baseline.clone();
        let report = calculate_psi("risk_score", &baseline, &current);
        assert!(report.total_psi < 1e-4);
        assert_eq!(report.status, DriftStatus::Green);
    }

    #[test]
    fn psi_drift_detection_flags_red() {
        // Baseline skewed low, current heavily shifted high (severe drift)
        let baseline = vec![0.05; 100];
        let current = vec![0.95; 100];
        let report = calculate_psi("risk_score", &baseline, &current);
        assert!(report.total_psi > PSI_MODERATE_THRESHOLD);
        assert_eq!(report.status, DriftStatus::Red);
    }
}
