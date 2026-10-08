//! Noisy-OR combination of weighted findings and decision banding.

use crate::config::Config;
use crate::types::{Decision, Finding, Rule};

/// Configured weight of `rule`. `Ml` findings are taken at face value (1.0).
pub fn weight(rule: Rule, cfg: &Config) -> f64 {
    match rule {
        Rule::Cycle => cfg.cycle.weight,
        Rule::MuleFanIn => cfg.fan_in.weight,
        Rule::PassThrough => cfg.pass_through.weight,
        Rule::FanOut => cfg.fan_out.weight,
        Rule::Velocity => cfg.velocity.weight,
        Rule::SharedIdentifier => cfg.shared_id.weight,
        Rule::Ml => 1.0,
    }
}

/// `score = 1 - Π(1 - wᵢ·sᵢ)`, banded into a decision. Sorts `findings` by
/// weighted contribution, strongest first (ties by rule for determinism).
pub fn evaluate(findings: &mut [Finding], cfg: &Config) -> (f64, Decision) {
    let contrib = |f: &Finding| (weight(f.rule, cfg) * f.score).clamp(0.0, 1.0);
    findings.sort_by(|a, b| contrib(b).total_cmp(&contrib(a)).then(a.rule.cmp(&b.rule)));
    let score = 1.0 - findings.iter().map(|f| 1.0 - contrib(f)).product::<f64>();
    (score, decide(score, cfg))
}

/// Map a combined score onto the configured bands.
pub fn decide(score: f64, cfg: &Config) -> Decision {
    if score >= cfg.bands.block {
        Decision::Block
    } else if score >= cfg.bands.review {
        Decision::Review
    } else {
        Decision::Allow
    }
}
