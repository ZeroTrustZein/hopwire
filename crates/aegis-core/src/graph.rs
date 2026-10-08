//! Time-windowed in-memory graph.
//!
//! - Interner: external string ID -> `u32` node index. The hash index stores only
//!   indices and borrows the ID from the node table, so each ID is stored once.
//! - Adjacency: per node, time-ordered (`ts` non-decreasing) deques for payments
//!   (out/in) and `Uses` links (out/in). Window counts are binary searches.
//! - Tx ids: refcounted string arena; a slot is freed (and reused) when the last
//!   edge carrying it is evicted.
//! - Eviction: edges with `ts < watermark - window_ms` are dropped from deque fronts
//!   whenever a node is touched, plus a small round-robin sweep per txn.

use std::collections::VecDeque;
use std::hash::BuildHasher;

use hashbrown::{DefaultHashBuilder, HashSet, HashTable};

use crate::types::{EdgeKind, NodeKind, PathEdge, SubNode, Subgraph};

/// Only the newest this-many `Uses` links are checked when refreshing a repeated link.
const USES_REFRESH_LOOKBACK: usize = 32;

/// Refcounted arena of tx-id strings with slot reuse.
#[derive(Debug, Default)]
pub struct TxStore {
    slots: Vec<(Box<str>, u32)>,
    free: Vec<u32>,
}

impl TxStore {
    /// Store `tx_id` with `refs` outstanding references; returns its slot.
    pub fn alloc(&mut self, tx_id: &str, refs: u32) -> u32 {
        match self.free.pop() {
            Some(i) => {
                self.slots[i as usize] = (tx_id.into(), refs);
                i
            }
            None => {
                self.slots.push((tx_id.into(), refs));
                (self.slots.len() - 1) as u32
            }
        }
    }

    pub fn get(&self, i: u32) -> &str {
        &self.slots[i as usize].0
    }

    /// Drop one reference; frees the string when none remain.
    pub fn release(&mut self, i: u32) {
        let slot = &mut self.slots[i as usize];
        slot.1 -= 1;
        if slot.1 == 0 {
            slot.0 = Box::default();
            self.free.push(i);
        }
    }

    /// Live (referenced) tx ids.
    #[cfg(test)]
    pub fn live(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}

/// Payment adjacency entry. `peer` is the creditor in an out-deque, the debtor in an in-deque.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentEdge {
    pub peer: u32,
    pub tx: u32,
    pub ts: i64,
    pub amount: i64,
}

/// `Uses` adjacency entry (account -> identifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsesEdge {
    pub peer: u32,
    pub tx: u32,
    pub ts: i64,
}

/// Anything with a timestamp, for ordered deque ops.
pub trait Timed {
    fn ts(&self) -> i64;
}

impl Timed for PaymentEdge {
    fn ts(&self) -> i64 {
        self.ts
    }
}

impl Timed for UsesEdge {
    fn ts(&self) -> i64 {
        self.ts
    }
}

/// Index of the first entry with `ts >= t`.
pub fn lower_bound<E: Timed>(dq: &VecDeque<E>, t: i64) -> usize {
    dq.partition_point(|e| e.ts() < t)
}

/// Insert keeping `ts` order; O(1) for in-order arrivals.
fn insert_sorted<E: Timed>(dq: &mut VecDeque<E>, e: E) {
    if dq.back().is_none_or(|b| b.ts() <= e.ts()) {
        dq.push_back(e);
    } else {
        let pos = dq.partition_point(|x| x.ts() <= e.ts());
        dq.insert(pos, e);
    }
}

/// Pop entries older than `cut` from the front, releasing their tx refs. Returns count.
fn evict_front<E: Timed + Copy>(
    dq: &mut VecDeque<E>,
    cut: i64,
    txs: &mut TxStore,
    tx: impl Fn(&E) -> u32,
) -> u64 {
    let mut n = 0;
    while let Some(e) = dq.front().copied().filter(|e| e.ts() < cut) {
        dq.pop_front();
        txs.release(tx(&e));
        n += 1;
    }
    if n > 0 && dq.is_empty() {
        dq.shrink_to_fit();
    }
    n
}

/// Bounded record of known counterparties per account: peer -> (first seen, last seen).
/// Outlives the 72-hour sliding window so repeat counterparties (payroll staff,
/// regular bills, standing orders) are not flagged as new.
///
/// When full, the least-recently-paid half is dropped in one O(n) pass (amortised
/// O(1) per insert). Recurring payees keep refreshing `last` and survive; one-offs
/// age out. FIFO eviction would do the opposite and churn a large payroll forever.
#[derive(Debug, Default)]
pub struct KnownPayees {
    pub(crate) map: hashbrown::HashMap<u32, (i64, i64)>,
}

// Covers normal employers and recurring billers while bounding per-account memory.
pub(crate) const KNOWN_PAYEES_CAP: usize = 16_384;

impl KnownPayees {
    /// Record a payment to `peer` at `ts`; keeps the original first-seen time.
    pub fn record(&mut self, peer: u32, ts: i64) {
        if let Some((_, last)) = self.map.get_mut(&peer) {
            *last = (*last).max(ts);
            return;
        }
        if self.map.len() >= KNOWN_PAYEES_CAP {
            let mut lasts: Vec<i64> = self.map.values().map(|v| v.1).collect();
            let mid = lasts.len() / 2;
            let cutoff = *lasts.select_nth_unstable(mid).1;
            self.map.retain(|_, v| v.1 > cutoff);
        }
        self.map.insert(peer, (ts, ts));
    }

    /// Whether `peer` is in the retained known set (retention outlives graph window).
    pub fn has_seen(&self, peer: u32) -> bool {
        self.map.contains_key(&peer)
    }

    /// Whether `peer` was first observed during the current scoring window.
    pub fn is_new(&self, peer: u32, cutoff: i64) -> bool {
        self.map
            .get(&peer)
            .is_none_or(|&(first_seen, _)| first_seen >= cutoff)
    }
}

#[derive(Debug)]
pub struct Node {
    pub id: Box<str>,
    pub kind: NodeKind,
    pub out: VecDeque<PaymentEdge>,
    pub inc: VecDeque<PaymentEdge>,
    pub uses_out: VecDeque<UsesEdge>,
    pub uses_in: VecDeque<UsesEdge>,
    /// Account opening time, if ever reported.
    pub opened_at_ms: Option<i64>,
    /// Payees this account has sent payments to.
    pub known_payees: KnownPayees,
    /// Payers this account has received payments from.
    pub known_payers: KnownPayees,
}

impl Node {
    fn new(id: &str, kind: NodeKind) -> Self {
        Self {
            id: id.into(),
            kind,
            out: VecDeque::new(),
            inc: VecDeque::new(),
            uses_out: VecDeque::new(),
            uses_in: VecDeque::new(),
            opened_at_ms: None,
            known_payees: KnownPayees::default(),
            known_payers: KnownPayees::default(),
        }
    }

    /// In+out degree (both edge kinds) counting only edges with `ts >= cut`.
    pub fn degree(&self, cut: i64) -> usize {
        fn live<E: Timed>(dq: &VecDeque<E>, cut: i64) -> usize {
            dq.len() - lower_bound(dq, cut)
        }
        live(&self.out, cut)
            + live(&self.inc, cut)
            + live(&self.uses_out, cut)
            + live(&self.uses_in, cut)
    }
}

#[derive(Debug)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub txs: TxStore,
    index: HashTable<u32>,
    hasher: DefaultHashBuilder,
    window_ms: i64,
    hub_degree: usize,
    /// Max timestamp seen; `i64::MIN` before the first txn.
    pub watermark: i64,
    /// Live edges, each counted once.
    pub edges: u64,
    sweep_cursor: usize,
}

impl Graph {
    pub fn new(window_ms: i64, hub_degree: u32) -> Self {
        Self {
            nodes: Vec::new(),
            txs: TxStore::default(),
            index: HashTable::new(),
            hasher: DefaultHashBuilder::default(),
            window_ms,
            hub_degree: hub_degree as usize,
            watermark: i64::MIN,
            edges: 0,
            sweep_cursor: 0,
        }
    }

    /// Edges with `ts` below this are outside the window.
    pub fn cutoff(&self) -> i64 {
        self.watermark.saturating_sub(self.window_ms)
    }

    pub fn advance(&mut self, ts: i64) {
        self.watermark = self.watermark.max(ts);
    }

    pub fn lookup(&self, id: &str) -> Option<u32> {
        let h = self.hasher.hash_one(id);
        self.index
            .find(h, |&i| &*self.nodes[i as usize].id == id)
            .copied()
    }

    /// Node index for `id`, creating it if new.
    pub fn intern(&mut self, id: &str, kind: NodeKind) -> u32 {
        let h = self.hasher.hash_one(id);
        let Self {
            index,
            nodes,
            hasher,
            ..
        } = self;
        if let Some(&i) = index.find(h, |&i| &*nodes[i as usize].id == id) {
            return i;
        }
        let i = nodes.len() as u32;
        nodes.push(Node::new(id, kind));
        index.insert_unique(h, i, |&j| hasher.hash_one(&*nodes[j as usize].id));
        i
    }

    pub fn node(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    pub fn node_mut(&mut self, i: u32) -> &mut Node {
        &mut self.nodes[i as usize]
    }

    pub fn is_hub(&self, i: u32) -> bool {
        self.node(i).degree(self.cutoff()) >= self.hub_degree
    }

    /// Add payment `from -> to`. The caller holds 2 refs on `tx` for it.
    pub fn add_payment(&mut self, from: u32, to: u32, ts: i64, amount: i64, tx: u32) {
        insert_sorted(
            &mut self.nodes[from as usize].out,
            PaymentEdge {
                peer: to,
                tx,
                ts,
                amount,
            },
        );
        insert_sorted(
            &mut self.nodes[to as usize].inc,
            PaymentEdge {
                peer: from,
                tx,
                ts,
                amount,
            },
        );
        self.edges += 1;
    }

    /// Add link `acct -> ident`, replacing a recent identical link so repeated use of
    /// the same device/ip refreshes one edge instead of piling up. Caller holds 2 refs on `tx`.
    pub fn add_uses(&mut self, acct: u32, ident: u32, ts: i64, tx: u32) {
        let recent = |dq: &VecDeque<UsesEdge>, peer: u32| {
            (dq.len().saturating_sub(USES_REFRESH_LOOKBACK)..dq.len())
                .rev()
                .find(|&p| dq[p].peer == peer)
        };
        if let Some(po) = recent(&self.nodes[acct as usize].uses_out, ident) {
            let old = self.nodes[acct as usize].uses_out[po].tx;
            let inc = &self.nodes[ident as usize].uses_in;
            let pi = (0..inc.len())
                .rev()
                .take(USES_REFRESH_LOOKBACK)
                .find(|&p| inc[p].peer == acct && inc[p].tx == old);
            if let Some(pi) = pi {
                self.nodes[acct as usize].uses_out.remove(po);
                self.nodes[ident as usize].uses_in.remove(pi);
                self.txs.release(old);
                self.txs.release(old);
                self.edges -= 1;
            }
        }
        insert_sorted(
            &mut self.nodes[acct as usize].uses_out,
            UsesEdge {
                peer: ident,
                tx,
                ts,
            },
        );
        insert_sorted(
            &mut self.nodes[ident as usize].uses_in,
            UsesEdge { peer: acct, tx, ts },
        );
        self.edges += 1;
    }

    /// Drop out-of-window edges of node `i`.
    pub fn evict(&mut self, i: u32) {
        let cut = self.cutoff();
        let Self { nodes, txs, .. } = self;
        let n = &mut nodes[i as usize];
        // Each edge lives in one out- and one in-deque; count it on the out side only.
        let gone = evict_front(&mut n.out, cut, txs, |e| e.tx)
            + evict_front(&mut n.uses_out, cut, txs, |e| e.tx);
        evict_front(&mut n.inc, cut, txs, |e| e.tx);
        evict_front(&mut n.uses_in, cut, txs, |e| e.tx);
        self.edges -= gone;
    }

    /// Evict `n` nodes round-robin so untouched nodes are cleaned too.
    pub fn sweep(&mut self, n: usize) {
        let len = self.nodes.len();
        for _ in 0..n.min(len) {
            self.sweep_cursor = (self.sweep_cursor + 1) % len;
            self.evict(self.sweep_cursor as u32);
        }
    }

    pub fn hubs(&self) -> u64 {
        let cut = self.cutoff();
        self.nodes
            .iter()
            .filter(|n| n.degree(cut) >= self.hub_degree)
            .count() as u64
    }

    fn path_edge(
        &self,
        kind: EdgeKind,
        from: u32,
        to: u32,
        tx: u32,
        ts: i64,
        amount: i64,
    ) -> PathEdge {
        PathEdge {
            kind,
            from: self.node(from).id.to_string(),
            to: self.node(to).id.to_string(),
            tx_id: self.txs.get(tx).to_string(),
            ts_ms: ts,
            amount_minor: amount,
        }
    }

    /// Out-payment `from -> e.peer` as a path edge.
    pub fn pay_edge(&self, from: u32, e: &PaymentEdge) -> PathEdge {
        self.path_edge(EdgeKind::Payment, from, e.peer, e.tx, e.ts, e.amount)
    }

    /// In-payment `e.peer -> to` as a path edge.
    pub fn pay_edge_in(&self, to: u32, e: &PaymentEdge) -> PathEdge {
        self.path_edge(EdgeKind::Payment, e.peer, to, e.tx, e.ts, e.amount)
    }

    /// In-link `e.peer -> ident` as a path edge.
    pub fn uses_edge(&self, ident: u32, e: &UsesEdge) -> PathEdge {
        self.path_edge(EdgeKind::Uses, e.peer, ident, e.tx, e.ts, 0)
    }

    fn sub_node(&self, i: u32) -> SubNode {
        let n = self.node(i);
        SubNode {
            id: n.id.to_string(),
            kind: n.kind,
            hub: self.is_hub(i),
        }
    }

    /// BFS neighbourhood over in+out edges of both kinds. Hubs other than the start are
    /// listed but not expanded. Order: BFS; per node out-payments, in-payments, out-links,
    /// in-links, each oldest first.
    pub fn subgraph(&self, id: &str, hops: u8, max_edges: usize) -> Subgraph {
        let Some(start) = self.lookup(id) else {
            return Subgraph::default();
        };
        let cut = self.cutoff();
        let (mut seen, mut seen_edges) = (HashSet::new(), HashSet::new());
        let mut sg = Subgraph {
            nodes: vec![self.sub_node(start)],
            edges: Vec::new(),
        };
        let mut queue = VecDeque::from([(start, 0u8)]);
        seen.insert(start);
        while let Some((cur, d)) = queue.pop_front() {
            if d >= hops || (cur != start && self.is_hub(cur)) {
                continue;
            }
            let n = self.node(cur);
            use EdgeKind::{Payment, Uses};
            let all = (n
                .out
                .iter()
                .map(|e| (cur, e.peer, Payment, e.tx, e.ts, e.amount)))
            .chain(
                n.inc
                    .iter()
                    .map(|e| (e.peer, cur, Payment, e.tx, e.ts, e.amount)),
            )
            .chain(
                n.uses_out
                    .iter()
                    .map(|e| (cur, e.peer, Uses, e.tx, e.ts, 0)),
            )
            .chain(n.uses_in.iter().map(|e| (e.peer, cur, Uses, e.tx, e.ts, 0)));
            for (from, to, kind, tx, ts, amount) in all {
                if ts < cut || !seen_edges.insert((from, to, kind, tx)) {
                    continue;
                }
                if sg.edges.len() >= max_edges {
                    return sg;
                }
                sg.edges
                    .push(self.path_edge(kind, from, to, tx, ts, amount));
                let peer = if from == cur { to } else { from };
                if seen.insert(peer) {
                    sg.nodes.push(self.sub_node(peer));
                    queue.push_back((peer, d + 1));
                }
            }
        }
        sg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_payees_keep_recurring_and_evict_stale() {
        let mut k = KnownPayees::default();
        // Payroll of 1000 staff paid every month; then a flood of one-off payees
        // larger than the cap. Staff must survive, the earliest one-offs must not.
        let payroll = 0..1000u32;
        let one_offs = 1_000_000..1_000_000 + KNOWN_PAYEES_CAP as u32;
        let mut ts = 0;
        for (i, p) in one_offs.clone().enumerate() {
            if i % 1000 == 0 {
                ts += 1;
                payroll.clone().for_each(|s| k.record(s, ts));
            }
            ts += 1;
            k.record(p, ts);
        }
        assert!(k.map.len() <= KNOWN_PAYEES_CAP);
        assert!(
            payroll.clone().all(|s| k.has_seen(s)),
            "recurring payees evicted"
        );
        assert!(!k.has_seen(one_offs.start), "stalest one-off kept");
        assert!(k.has_seen(one_offs.end - 1), "newest one-off evicted");
        assert!(!k.is_new(0, 2), "first-seen time must not move on repeat");
    }

    #[test]
    fn eviction_frees_tx_ids_and_edges() {
        let mut g = Graph::new(1_000, 500);
        let (a, b) = (
            g.intern("a", NodeKind::Account),
            g.intern("b", NodeKind::Account),
        );
        let dev = g.intern("device:x", NodeKind::Device);
        for ts in 0..10 {
            g.advance(ts);
            let tx = g.txs.alloc(&format!("t{ts}"), 4);
            g.add_payment(a, b, ts, 100, tx);
            g.add_uses(a, dev, ts, tx);
        }
        // Repeated device use is refreshed, not accumulated.
        assert_eq!(g.node(a).uses_out.len(), 1);
        assert_eq!(g.edges, 11);
        assert_eq!(g.txs.live(), 10);
        g.advance(5_000);
        g.sweep(10);
        assert_eq!(g.edges, 0);
        assert_eq!(g.txs.live(), 0);
        // Freed slots are reused.
        let tx = g.txs.alloc("new", 2);
        assert!(tx < 10);
    }

    #[test]
    fn interner_is_stable() {
        let mut g = Graph::new(1_000, 500);
        let a = g.intern("a", NodeKind::Account);
        assert_eq!(g.intern("a", NodeKind::Account), a);
        assert_eq!(g.lookup("a"), Some(a));
        assert_eq!(g.lookup("zz"), None);
    }
}
