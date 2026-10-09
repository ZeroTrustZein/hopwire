//! Public data contract. Shared by engine, server, bench and (via JSON) console.
//! Changing anything here is a breaking change — coordinate before editing.

use serde::{Deserialize, Serialize};

/// Payment scheme. UK-first; `Other` covers everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Fps,
    Chaps,
    Bacs,
    Card,
    Internal,
    #[default]
    Other,
}

/// One payment. Field set modelled on ISO 20022 pacs.008 essentials.
/// Account IDs are opaque strings (e.g. sort code + account number, IBAN, or hashed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Txn {
    pub tx_id: String,
    /// Unix epoch milliseconds.
    pub ts_ms: i64,
    /// Amount in minor units (pence/cents). Always > 0.
    pub amount_minor: i64,
    /// ISO 4217, e.g. "GBP".
    pub currency: String,
    pub debtor: String,
    pub creditor: String,
    #[serde(default)]
    pub scheme: Scheme,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    /// Hashed customer identity (e.g. sha256 of national ID) of the debtor.
    #[serde(default)]
    pub debtor_customer_hash: Option<String>,
    #[serde(default)]
    pub creditor_customer_hash: Option<String>,
    /// When the creditor account was opened (epoch ms), if known. Young accounts = mule risk.
    #[serde(default)]
    pub creditor_account_opened_at_ms: Option<i64>,
}

/// Node kinds in the graph. External node IDs in paths/subgraphs are strings:
/// accounts are the raw account ID; other kinds are prefixed:
/// `device:<id>`, `ip:<addr>`, `cust:<hash>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Account,
    Device,
    Ip,
    Customer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// Money moved `from` -> `to`.
    Payment,
    /// Account `from` used/owns identifier node `to` (device, ip, customer).
    Uses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    Cycle,
    MuleFanIn,
    PassThrough,
    FanOut,
    Velocity,
    SharedIdentifier,
    /// Reserved for the ML path (TGN/ONNX). Never emitted by the community core.
    Ml,
}

/// One edge in an explanation path or subgraph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathEdge {
    pub kind: EdgeKind,
    pub from: String,
    pub to: String,
    /// The transaction that created this edge.
    pub tx_id: String,
    pub ts_ms: i64,
    /// 0 for `Uses` edges.
    pub amount_minor: i64,
}

/// A detector hit. `path` is the human-auditable explanation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Rule,
    /// Detector confidence in [0, 1].
    pub score: f64,
    /// One-sentence plain-English reason, suitable for an analyst / SAR narrative.
    pub reason: String,
    pub path: Vec<PathEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Review,
    Block,
}

/// Result of scoring one transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub tx_id: String,
    /// Combined score in [0, 1]: noisy-OR over weighted findings.
    pub score: f64,
    pub decision: Decision,
    pub findings: Vec<Finding>,
    /// Engine-internal processing time in microseconds.
    pub latency_us: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubNode {
    pub id: String,
    pub kind: NodeKind,
    pub hub: bool,
}

/// Neighbourhood snapshot for the investigator console.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Subgraph {
    pub nodes: Vec<SubNode>,
    pub edges: Vec<PathEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EngineStats {
    pub nodes: u64,
    pub edges: u64,
    pub hubs: u64,
    pub processed: u64,
    /// Latest transaction timestamp seen (drives window eviction).
    pub watermark_ms: i64,
}
