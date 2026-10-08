//! Design-partner replay: POST a txn file to a running aegis-server.
//! Batch mode -> POST /v1/transactions (Txn[] chunks); --sync -> POST /v1/score per txn
//! with an end-to-end HTTP latency histogram.

use std::path::Path;
use std::time::{Duration, Instant};

use aegis_core::Txn;
use hdrhistogram::Histogram;
use serde_json::Value;

use crate::{io, Res};

pub fn replay(
    target: &str,
    key: &str,
    input: &Path,
    batch: usize,
    rps: Option<f64>,
    sync: bool,
) -> Res<()> {
    let base = target.trim_end_matches('/');
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;
    let mut hist = Histogram::<u64>::new_with_bounds(1, 600_000_000, 3)?;
    let (mut sent, mut alerts, mut failed) = (0u64, 0u64, 0u64);
    let start = Instant::now();

    // POST JSON `body` covering `n` txns; returns the response JSON on 2xx.
    let mut post = |path: &str, body: Vec<u8>, n: u64| -> Res<Option<Value>> {
        let t = Instant::now();
        let resp = client
            .post(format!("{base}{path}"))
            .bearer_auth(key)
            .header("content-type", "application/json")
            .body(body)
            .send()?;
        let us = t.elapsed().as_micros() as u64;
        let status = resp.status();
        let v: Value = resp.json().unwrap_or(Value::Null);
        if status.as_u16() == 401 {
            return Err("401 unauthorized: check --api-key / AEGIS_API_KEY".into());
        }
        if !status.is_success() {
            failed += n;
            eprintln!("{path}: HTTP {status}: {v}");
            return Ok(None);
        }
        hist.saturating_record(us.max(1));
        sent += n;
        if let Some(r) = rps {
            if let Some(wait) =
                Duration::from_secs_f64(sent as f64 / r).checked_sub(start.elapsed())
            {
                std::thread::sleep(wait);
            }
        }
        Ok(Some(v))
    };

    let batch = batch.clamp(1, 10_000);
    let mut chunk: Vec<Txn> = Vec::with_capacity(batch);
    let mut txns = io::read_txns(input)?.peekable();
    while let Some(t) = txns.next() {
        if sync {
            if let Some(v) = post("/v1/score", serde_json::to_vec(&t)?, 1)? {
                alerts += u64::from(!v["alert_id"].is_null());
            }
            continue;
        }
        chunk.push(t);
        if chunk.len() == batch || txns.peek().is_none() {
            if let Some(v) = post(
                "/v1/transactions",
                serde_json::to_vec(&chunk)?,
                chunk.len() as u64,
            )? {
                alerts += v["alerts"].as_u64().unwrap_or(0);
            }
            chunk.clear();
        }
    }

    let secs = start.elapsed().as_secs_f64();
    println!(
        "replayed {sent} txns in {secs:.2}s ({:.0} TPS), {alerts} alerts, {failed} failed",
        sent as f64 / secs
    );
    if !hist.is_empty() {
        let q = |p| hist.value_at_quantile(p) as f64 / 1e3;
        println!(
            "HTTP latency per {} (ms): p50={:.2} p90={:.2} p99={:.2} p99.9={:.2} max={:.2}",
            if sync { "request" } else { "batch" },
            q(0.5),
            q(0.9),
            q(0.99),
            q(0.999),
            hist.max() as f64 / 1e3
        );
    }
    Ok(())
}
