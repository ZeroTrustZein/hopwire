//! aegis-bench: synthetic + IBM AML benchmark, recall/latency report, and HTTP replay tool.

mod ibm;
mod io;
mod replay;
mod run;
mod synth;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

pub type Res<T> = Result<T, Box<dyn std::error::Error>>;

/// Ground truth for one laundering transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub typology: String,
    /// Laundering attempt the txn belongs to (None = labelled txn without a known attempt).
    pub attempt: Option<u64>,
}

#[derive(Parser)]
#[command(about = "AegisGraph benchmark and replay tool")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct RunOpts {
    /// Engine config TOML (defaults used if omitted).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Markdown report output.
    #[arg(long, default_value = "bench/report.md")]
    report: PathBuf,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate seeded synthetic traffic with injected typologies. Runs the engine on it,
    /// or with --out writes txns (CSV, or JSONL if the path ends in .jsonl) + <out>.labels.csv.
    Synth {
        #[arg(long, default_value_t = 10_000)]
        accounts: usize,
        #[arg(long, default_value_t = 30)]
        days: u32,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, default_value_t = 20)]
        rings_per_type: u32,
        #[arg(long)]
        out: Option<PathBuf>,
        #[command(flatten)]
        run: RunOpts,
    },
    /// Run the engine over the IBM "Realistic Synthetic Financial Transactions for AML" dataset.
    Ibm {
        #[arg(long)]
        trans: PathBuf,
        #[arg(long)]
        patterns: PathBuf,
        /// Reorder buffer (rows) used to sort the stream by timestamp.
        #[arg(long, default_value_t = 1_000_000)]
        reorder: usize,
        /// Stop after this many rows.
        #[arg(long)]
        limit: Option<u64>,
        #[command(flatten)]
        run: RunOpts,
    },
    /// Run the engine over a txn file (CSV/JSONL) with optional labels CSV (tx_id,typology,attempt).
    Run {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        labels: Option<PathBuf>,
        #[command(flatten)]
        run: RunOpts,
    },
    /// Replay a txn file (CSV/JSONL) against a running aegis-server.
    Replay {
        #[arg(long)]
        target: String,
        #[arg(long, env = "AEGIS_API_KEY", hide_env_values = true)]
        api_key: String,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, default_value_t = 1000)]
        batch: usize,
        /// Max transactions per second.
        #[arg(long)]
        rps: Option<f64>,
        /// One-by-one POST /v1/score, measuring end-to-end latency.
        #[arg(long)]
        sync: bool,
    },
}

fn load_config(path: &Option<PathBuf>) -> Res<aegis_core::Config> {
    Ok(match path {
        Some(p) => aegis_core::Config::from_toml(&std::fs::read_to_string(p)?)?,
        None => aegis_core::Config::default(),
    })
}

fn main() -> Res<()> {
    match Cli::parse().cmd {
        Cmd::Synth {
            accounts,
            days,
            seed,
            rings_per_type,
            out,
            run,
        } => {
            let gen = synth::Synth::new(accounts, days, seed, rings_per_type);
            match out {
                Some(path) => io::write_synth(gen, &path),
                None => {
                    let ds = format!(
                        "synthetic (accounts={accounts}, days={days}, seed={seed}, rings_per_type={rings_per_type})"
                    );
                    run::run(load_config(&run.config)?, &ds, gen, &run.report)
                }
            }
        }
        Cmd::Ibm {
            trans,
            patterns,
            reorder,
            limit,
            run,
        } => {
            let pats = ibm::Patterns::load(&patterns)?;
            let stream = ibm::IbmStream::open(&trans, pats, reorder)?;
            let ds = format!("IBM AML {}", trans.display());
            let items = stream.take(limit.unwrap_or(u64::MAX) as usize);
            run::run(load_config(&run.config)?, &ds, items, &run.report)
        }
        Cmd::Run { input, labels, run } => {
            let labels = labels
                .map(|p| io::read_labels(&p))
                .transpose()?
                .unwrap_or_default();
            let items = io::read_txns(&input)?.map(move |t| {
                let l = labels.get(&t.tx_id).cloned();
                (t, l)
            });
            run::run(
                load_config(&run.config)?,
                &input.display().to_string(),
                items,
                &run.report,
            )
        }
        Cmd::Replay {
            target,
            api_key,
            input,
            batch,
            rps,
            sync,
        } => replay::replay(&target, &api_key, &input, batch, rps, sync),
    }
}
