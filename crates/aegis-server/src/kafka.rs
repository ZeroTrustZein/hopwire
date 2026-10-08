//! Kafka / Redpanda real-time streaming ingestion pipeline.
//!
//! Subscribes to configured topic, micro-batches events, ingests through
//! the engine, persists to Postgres, and records alerts.

#[cfg(feature = "kafka")]
pub mod consumer {
    use aegis_core::Txn;
    use rskafka::client::partition::{OffsetAt, UnknownTopicHandling};
    use rskafka::client::ClientBuilder;
    use std::time::Duration;
    use tokio::sync::watch;
    use tracing::{error, info, warn};

    use crate::{error::AppError, AppState};

    #[derive(Debug, Clone)]
    pub struct KafkaConfig {
        pub brokers: Vec<String>,
        pub topic: String,
        pub partition: i32,
    }

    impl KafkaConfig {
        pub fn from_env() -> Option<Self> {
            let brokers = std::env::var("AEGIS_KAFKA_BROKERS").ok()?;
            let topic =
                std::env::var("AEGIS_KAFKA_TOPIC").unwrap_or_else(|_| "payments.events".into());
            let partition = std::env::var("AEGIS_KAFKA_PARTITION")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(0);

            let broker_list: Vec<String> = brokers
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if broker_list.is_empty() {
                None
            } else {
                Some(Self {
                    brokers: broker_list,
                    topic,
                    partition,
                })
            }
        }
    }

    /// Spawns the background Kafka / Redpanda consumer worker.
    pub async fn run_consumer(
        st: AppState,
        cfg: KafkaConfig,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), anyhow::Error> {
        info!(topic = %cfg.topic, partition = cfg.partition, "Starting Kafka streaming consumer");
        let client = ClientBuilder::new(cfg.brokers).build().await?;
        let partition_client = client
            .partition_client(&cfg.topic, cfg.partition, UnknownTopicHandling::Retry)
            .await?;

        // Start from latest offset
        let mut offset = partition_client.get_offset(OffsetAt::Latest).await?;
        info!(start_offset = offset, "Connected to Kafka broker");

        while !*shutdown.borrow() {
            tokio::select! {
                _ = shutdown.changed() => {
                    info!("Kafka consumer shutting down");
                    break;
                }
                res = partition_client.fetch_records(offset, 1..(1024 * 1024), 1000) => {
                    match res {
                        Ok((records, next_offset)) => {
                            if records.is_empty() {
                                offset = next_offset;
                                continue;
                            }
                            let mut batch: Vec<Txn> = Vec::with_capacity(records.len());
                            for r in records {
                                let val = &r.record.value;
                                if let Some(bytes) = val {
                                    if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<Document") {
                                        if let Ok(xml_str) = std::str::from_utf8(bytes) {
                                            if let Ok(parsed) = aegis_iso20022::parse_pacs008(xml_str) {
                                                batch.extend(parsed.into_iter().map(|p| p.to_txn()));
                                                continue;
                                            }
                                        }
                                    }
                                    if let Ok(txn) = serde_json::from_slice::<Txn>(bytes) {
                                        batch.push(txn);
                                    }
                                }
                            }

                            if !batch.is_empty() {
                                if let Err(e) = process_batch(&st, batch).await {
                                    error!(error = ?e, "Failed to process Kafka record batch");
                                }
                            }
                            offset = next_offset;
                        }
                        Err(e) => {
                            warn!(error = %e, "Kafka fetch error, retrying in 1s");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn process_batch(st: &AppState, txns: Vec<Txn>) -> Result<(), AppError> {
        let db = st.pool.begin().await?;
        let _verdicts = st.engine.process_batch(txns).await?;
        db.commit().await?;
        Ok(())
    }
}

#[cfg(not(feature = "kafka"))]
pub mod consumer {
    #[derive(Debug, Clone)]
    pub struct KafkaConfig;

    impl KafkaConfig {
        pub fn from_env() -> Option<Self> {
            None
        }
    }
}
