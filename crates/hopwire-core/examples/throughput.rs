//! Single-thread throughput/latency check on random traffic.
//! `cargo run -p hopwire-core --release --example throughput [txns] [accounts]`

use std::time::Instant;

use hopwire_core::{Config, Engine, Txn};

fn main() {
    let mut args = std::env::args()
        .skip(1)
        .map(|a| a.parse::<u64>().expect("number"));
    let n = args.next().unwrap_or(1_000_000);
    let accounts = args.next().unwrap_or(10_000);
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    // ~1M txns over ~30 days: one txn every ~2.6s on average.
    let txns: Vec<Txn> = (0..n)
        .map(|i| {
            let d = next() % accounts;
            let c = next() % accounts;
            let r = next();
            Txn {
                tx_id: format!("tx{i:08}"),
                ts_ms: 1_700_000_000_000 + i as i64 * 2_600,
                amount_minor: 100 + (r % 500_000) as i64,
                currency: "GBP".into(),
                debtor: format!("acct{d}"),
                creditor: format!("acct{}", if c == d { (c + 1) % accounts } else { c }),
                scheme: Default::default(),
                device_id: (r % 3 == 0).then(|| format!("dev{}", d / 2)),
                ip: (r % 5 == 0).then(|| format!("10.0.{}.{}", (d / 256) % 256, d % 256)),
                debtor_customer_hash: Some(format!("c{d}")),
                creditor_customer_hash: None,
                creditor_account_opened_at_ms: None,
            }
        })
        .collect();

    let mut engine = Engine::new(Config::default());
    let mut lat = Vec::with_capacity(n as usize);
    let mut flagged = [0u64; 3];
    let start = Instant::now();
    for t in &txns {
        let t0 = Instant::now();
        let v = engine.process(t);
        lat.push(t0.elapsed().as_nanos() as u64);
        flagged[v.decision as usize] += 1;
    }
    let secs = start.elapsed().as_secs_f64();
    lat.sort_unstable();
    let pct = |p: f64| lat[((lat.len() - 1) as f64 * p) as usize] as f64 / 1_000.0;
    println!("txns {n}, accounts {accounts}, {:.0} TPS", n as f64 / secs);
    println!(
        "latency us: p50 {:.2}  p99 {:.2}  p99.9 {:.2}  max {:.2}",
        pct(0.5),
        pct(0.99),
        pct(0.999),
        pct(1.0)
    );
    println!("decisions allow/review/block: {flagged:?}");
    println!("{:?}", engine.stats());
}
