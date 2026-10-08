//! AegisGraph Core Engine
//! In-memory, time-windowed graph and deterministic AML detectors.
//! Zero IO, zero network dependencies.

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_test() {
        assert_eq!(version(), "0.1.0");
    }
}
