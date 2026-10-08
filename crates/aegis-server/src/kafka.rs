//! Kafka/Redpanda ingest seam (feature `kafka`). Not implemented in the pilot.
//!
//! A future consumer reads `Txn` JSON from a topic and feeds the same path as
//! `POST /v1/transactions` (persist → `EngineHandle::process_batch` → alerts +
//! audit), committing offsets only after the Postgres transaction commits.
//! Restart recovery stays Postgres-driven (`db::rebuild`), not offset-driven.
