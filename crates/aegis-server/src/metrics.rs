use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Global Prometheus recorder (installed once per process; tests build many apps).
pub fn handle() -> PrometheusHandle {
    HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .set_buckets_for_metric(
                    Matcher::Full("aegis_score_latency_us".into()),
                    &[
                        10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 10_000.0,
                    ],
                )
                .expect("buckets")
                .install_recorder()
                .expect("install Prometheus recorder")
        })
        .clone()
}
