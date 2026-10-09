//! Hopwire Enterprise Modules (EE).
//!
//! Proprietary enterprise modules:
//! - Automated Regulatory SAR Narrative generation (FinCEN / UK FIU / goAML).
//! - Granular Role-Based Access Control (RBAC) & OIDC authentication.
//! - Probabilistic Entity Resolution and cross-account consortium risk.

pub mod entity_resolution;
pub mod rbac;
pub mod sar;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
