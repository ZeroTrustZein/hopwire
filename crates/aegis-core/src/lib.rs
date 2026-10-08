//! AegisGraph Core Engine
//! In-memory, time-windowed graph and deterministic AML detectors.
//! Zero IO, zero network dependencies.

pub mod config;
mod detectors;
pub mod drift;
mod engine;
mod graph;
pub mod ml;
mod score;
pub mod types;

pub use config::Config;
pub use engine::{Engine, MlScorer};
pub use types::*;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
