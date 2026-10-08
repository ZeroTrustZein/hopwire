//! Single-writer engine actor. One OS thread owns `aegis_core::Engine`; every
//! request goes through one mpsc queue, so processing order is deterministic.

use aegis_core::{Config, Engine, EngineStats, Subgraph, Txn, Verdict};
use tokio::sync::{mpsc, oneshot};

use anyhow::{anyhow, Result};

enum Cmd {
    Process(Txn, oneshot::Sender<Verdict>),
    ProcessBatch(Vec<Txn>, oneshot::Sender<Vec<Verdict>>),
    Ingest(Vec<Txn>, oneshot::Sender<()>),
    Subgraph { node_ids: Vec<String>, hops: u8, max_edges: usize, reply: oneshot::Sender<Vec<Subgraph>> },
    Stats(oneshot::Sender<EngineStats>),
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<Cmd>,
}

impl EngineHandle {
    pub fn spawn(cfg: Config) -> Self {
        let (tx, mut rx) = mpsc::channel::<Cmd>(1024);
        std::thread::Builder::new()
            .name("aegis-engine".into())
            .spawn(move || {
                let mut engine = Engine::new(cfg);
                while let Some(cmd) = rx.blocking_recv() {
                    match cmd {
                        Cmd::Process(t, r) => {
                            let _ = r.send(engine.process(&t));
                        }
                        Cmd::ProcessBatch(ts, r) => {
                            let _ = r.send(ts.iter().map(|t| engine.process(t)).collect());
                        }
                        Cmd::Ingest(ts, r) => {
                            ts.iter().for_each(|t| engine.ingest(t));
                            let _ = r.send(());
                        }
                        Cmd::Subgraph { node_ids, hops, max_edges, reply } => {
                            let _ = reply.send(
                                node_ids.iter().map(|n| engine.subgraph(n, hops, max_edges)).collect(),
                            );
                        }
                        Cmd::Stats(r) => {
                            let _ = r.send(engine.stats());
                        }
                    }
                }
            })
            .expect("spawn engine thread");
        Self { tx }
    }

    async fn call<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Cmd) -> Result<T> {
        let (r, w) = oneshot::channel();
        self.tx.send(make(r)).await.map_err(|_| unavailable())?;
        w.await.map_err(|_| unavailable())
    }

    pub async fn process(&self, t: Txn) -> Result<Verdict> {
        self.call(|r| Cmd::Process(t, r)).await
    }

    pub async fn process_batch(&self, ts: Vec<Txn>) -> Result<Vec<Verdict>> {
        self.call(|r| Cmd::ProcessBatch(ts, r)).await
    }

    pub async fn ingest(&self, ts: Vec<Txn>) -> Result<()> {
        self.call(|r| Cmd::Ingest(ts, r)).await
    }

    pub async fn subgraphs(&self, node_ids: Vec<String>, hops: u8, max_edges: usize) -> Result<Vec<Subgraph>> {
        self.call(|reply| Cmd::Subgraph { node_ids, hops, max_edges, reply }).await
    }

    pub async fn stats(&self) -> Result<EngineStats> {
        self.call(Cmd::Stats).await
    }
}

fn unavailable() -> anyhow::Error {
    anyhow!("engine actor unavailable (thread exited)")
}
