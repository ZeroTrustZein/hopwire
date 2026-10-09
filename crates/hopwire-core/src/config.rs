//! Engine configuration. Loaded from TOML; every field has a default so a
//! partial file (or none) works.

use serde::{Deserialize, Serialize};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid config TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Edges older than `watermark - window_ms` are evicted.
    pub window_ms: i64,
    /// How much history to replay on restart (>= `window_ms`). Edges beyond `window_ms`
    /// are evicted again, but counterparty memory (known payees/payers) survives, so a
    /// restart does not forget a monthly payroll. Should cover the longest pay cycle.
    pub history_ms: i64,
    /// In+out degree (within window) at which a node is tagged HUB and no longer expanded.
    pub hub_degree: u32,
    pub cycle: CycleCfg,
    pub fan_in: FanInCfg,
    pub pass_through: PassThroughCfg,
    pub fan_out: FanOutCfg,
    pub velocity: VelocityCfg,
    pub shared_id: SharedIdCfg,
    pub bands: Bands,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CycleCfg {
    pub enabled: bool,
    pub weight: f64,
    /// Max cycle length in hops (2..=4).
    pub max_hops: u8,
    /// Max edge visits per search; bounds worst-case latency.
    pub budget: u32,
    /// Cycle must complete within this span (first to last edge).
    pub max_span_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FanInCfg {
    pub enabled: bool,
    pub weight: f64,
    pub window_ms: i64,
    pub min_senders: u32,
    /// Accounts opened within this period get a score boost.
    pub young_account_ms: i64,
    /// Established accounts need at least this many new payers in the window.
    pub min_new_senders: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PassThroughCfg {
    pub enabled: bool,
    pub weight: f64,
    /// Outflow must follow inflow within this delay.
    pub within_ms: i64,
    /// Outflow / recent inflow ratio that triggers.
    pub min_ratio: f64,
    /// Ignore tiny flows.
    pub min_inflow_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FanOutCfg {
    pub enabled: bool,
    pub weight: f64,
    pub window_ms: i64,
    pub min_payees: u32,
    /// Payees opened within this period boost the fan-out score.
    pub young_payee_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VelocityCfg {
    pub enabled: bool,
    pub weight: f64,
    pub window_ms: i64,
    /// Outgoing txn count per account per window.
    pub max_count: u32,
    /// Outgoing sum per account per window (minor units).
    pub max_sum_minor: i64,
    /// Payees opened within this period count toward velocity. Unknown opening dates count too.
    pub young_payee_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedIdCfg {
    pub enabled: bool,
    pub weight: f64,
    /// Identifier (device/ip/customer) linked to at least this many distinct accounts.
    pub min_accounts: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bands {
    /// score >= review -> Review
    pub review: f64,
    /// score >= block -> Block
    pub block: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_ms: 72 * HOUR,
            history_ms: 35 * DAY,
            hub_degree: 500,
            cycle: CycleCfg::default(),
            fan_in: FanInCfg::default(),
            pass_through: PassThroughCfg::default(),
            fan_out: FanOutCfg::default(),
            velocity: VelocityCfg::default(),
            shared_id: SharedIdCfg::default(),
            bands: Bands::default(),
        }
    }
}

impl Default for CycleCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.9,
            max_hops: 4,
            budget: 2_000,
            max_span_ms: 72 * HOUR,
        }
    }
}

impl Default for FanInCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.58,
            window_ms: 24 * HOUR,
            min_senders: 6,
            young_account_ms: 30 * DAY,
            min_new_senders: 6,
        }
    }
}

impl Default for PassThroughCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.7,
            within_ms: 60 * MIN,
            min_ratio: 0.8,
            min_inflow_minor: 10_000,
        }
    }
}

impl Default for FanOutCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.6,
            window_ms: 24 * HOUR,
            min_payees: 10,
            young_payee_ms: 30 * DAY,
        }
    }
}

impl Default for VelocityCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.7,
            window_ms: HOUR,
            max_count: 15,
            max_sum_minor: 50_000_000,
            young_payee_ms: 30 * DAY,
        }
    }
}

impl Default for SharedIdCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            weight: 0.85,
            min_accounts: 4,
        }
    }
}

impl Default for Bands {
    fn default() -> Self {
        Self {
            review: 0.5,
            block: 0.85,
        }
    }
}

impl Config {
    pub fn from_toml(s: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(s)?)
    }
}
