//! Deterministic synthetic payment traffic with injected, labelled laundering typologies.
//!
//! Background (heavy-tailed): payroll hubs (monthly/weekly BACS fan-out), card spend to merchants
//! with Pareto popularity, P2P within small social circles, monthly bills. Generated one day at a
//! time so memory stays bounded; injected typology txns (precomputed, small) are merged in.

use aegis_core::{Scheme, Txn};

use crate::Label;

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
/// 2026-01-01T00:00:00Z (a Thursday).
const T0: i64 = 1_767_225_600_000;
const POUND: f64 = 100.0;

pub const TYPOLOGIES: [&str; 7] = [
    "CYCLE",
    "FAN-IN",
    "PASS-THROUGH",
    "FAN-OUT",
    "SCATTER-GATHER",
    "SHARED-DEVICE",
    "VELOCITY",
];

/// SplitMix64: tiny, fast, and stable across crate versions (determinism is the point).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform in [lo, hi).
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo).max(1) as u64) as i64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    /// Heavy-tailed index in [0, n): low indices are much more popular.
    fn skewed(&mut self, n: usize, power: f64) -> usize {
        ((n as f64 * self.f().powf(power)) as usize).min(n - 1)
    }
    fn normal(&mut self) -> f64 {
        let u = self.f().max(1e-12);
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * self.f()).cos()
    }
    /// Log-normal amount in minor units around `median_pounds`.
    fn amount(&mut self, median_pounds: f64, sigma: f64) -> i64 {
        ((median_pounds * POUND * (sigma * self.normal()).exp()) as i64).max(100)
    }
    fn poisson(&mut self, lambda: f64) -> u32 {
        let (l, mut k, mut p) = ((-lambda).exp(), 0, 1.0);
        loop {
            p *= self.f();
            if p <= l {
                return k;
            }
            k += 1;
        }
    }
}

#[derive(Clone, Copy)]
struct Ev {
    ts: i64,
    from: u32,
    to: u32,
    amount: i64,
    scheme: Scheme,
    device: Option<u32>,
    /// (typology index, attempt id)
    label: Option<(u8, u32)>,
}

struct World {
    n_emp: usize,
    n_merch: usize,
    n_bg: usize,
    /// Per account: opened-at ms and device id.
    opened: Vec<i64>,
    device: Vec<u32>,
    /// Per person (index - persons_lo): employer, salary, friends.
    employer: Vec<u32>,
    salary: Vec<i64>,
    friends: Vec<[u32; 8]>,
    weekly: Vec<bool>,
}

impl World {
    fn persons_lo(&self) -> usize {
        self.n_emp + self.n_merch
    }
    fn person(&self, rng: &mut Rng) -> u32 {
        (self.persons_lo() + rng.below(self.n_bg - self.persons_lo())) as u32
    }
    fn persons(&self, rng: &mut Rng, k: usize) -> Vec<u32> {
        let mut v: Vec<u32> = Vec::with_capacity(k);
        while v.len() < k {
            let p = self.person(rng);
            if !v.contains(&p) {
                v.push(p);
            }
        }
        v
    }
    /// New ring account (own device), opened at `opened`.
    fn fresh(&mut self, opened: i64) -> u32 {
        let id = self.opened.len() as u32;
        self.opened.push(opened);
        self.device.push(id);
        id
    }
}

pub struct Synth {
    rng: Rng,
    world: World,
    days: u32,
    day: u32,
    /// Injected events, sorted by ts; `inj_pos` is the merge cursor.
    inj: Vec<Ev>,
    inj_pos: usize,
    buf: std::vec::IntoIter<Ev>,
    seq: u64,
}

impl Synth {
    pub fn new(accounts: usize, days: u32, seed: u64, rings_per_type: u32) -> Self {
        assert!(accounts >= 200, "--accounts must be >= 200");
        assert!(days >= 3, "--days must be >= 3");
        let mut rng = Rng(seed);
        let n_emp = (accounts / 1000).max(2);
        let n_merch = (accounts / 50).max(10);
        let n_persons = accounts - n_emp - n_merch;
        let lo = n_emp + n_merch;
        let mut opened = Vec::with_capacity(accounts);
        let mut device = Vec::with_capacity(accounts);
        for i in 0..accounts {
            opened.push(T0 - rng.range(180, 3650) * DAY);
            // ~5% of persons share a household device with the previous account.
            let shared = i > lo && rng.f() < 0.05;
            device.push(if shared { device[i - 1] } else { i as u32 });
        }
        let mut employer = Vec::with_capacity(n_persons);
        let mut salary = Vec::with_capacity(n_persons);
        let mut friends = Vec::with_capacity(n_persons);
        for i in 0..n_persons {
            employer.push(rng.skewed(n_emp, 2.0) as u32);
            salary.push(rng.amount(2400.0, 0.4));
            let mut fr = [0u32; 8];
            for (k, f) in fr.iter_mut().enumerate() {
                // Half local (clustered circles, some reciprocity), half anywhere.
                let p = if k < 4 {
                    (i as i64 + rng.range(-50, 51)).clamp(0, n_persons as i64 - 1) as usize
                } else {
                    rng.below(n_persons)
                };
                *f = (lo + p) as u32;
            }
            friends.push(fr);
        }
        let weekly = (0..n_emp).map(|_| rng.f() < 0.3).collect();
        let world = World {
            n_emp,
            n_merch,
            n_bg: accounts,
            opened,
            device,
            employer,
            salary,
            friends,
            weekly,
        };
        let mut s = Self {
            rng,
            world,
            days,
            day: 0,
            inj: Vec::new(),
            inj_pos: 0,
            buf: Vec::new().into_iter(),
            seq: 0,
        };
        s.inject(rings_per_type);
        s
    }

    fn inject(&mut self, rings: u32) {
        let mut attempt = 0u32;
        for typ in 0..TYPOLOGIES.len() as u8 {
            for _ in 0..rings {
                attempt += 1;
                let rng = &mut self.rng;
                let day = rng.range(1, self.days as i64 - 1);
                let t0 = T0 + day * DAY + rng.range(8 * HOUR, 20 * HOUR);
                let label = Some((typ, attempt));
                let w = &mut self.world;
                let ev = |ts, from: u32, to, amount, dev: &[u32]| Ev {
                    ts,
                    from,
                    to,
                    amount,
                    scheme: Scheme::Fps,
                    device: Some(dev[from as usize]),
                    label,
                };
                let young = |rng: &mut Rng| t0 - rng.range(1, 20) * DAY;
                let out = &mut self.inj;
                match TYPOLOGIES[typ as usize] {
                    "CYCLE" => {
                        let k = 2 + rng.below(3);
                        let m = w.persons(rng, k);
                        let (mut t, mut amt) = (t0, rng.amount(15_000.0, 0.5));
                        for i in 0..k {
                            if i > 0 {
                                t += rng.range(10 * MIN, 4 * HOUR);
                                amt = amt * (100 - rng.range(1, 6)) / 100;
                            }
                            out.push(ev(t, m[i], m[(i + 1) % k], amt, &w.device));
                        }
                    }
                    "FAN-IN" => {
                        let mule = w.fresh(young(rng));
                        let k = 10 + rng.below(21);
                        for s in w.persons(rng, k) {
                            out.push(ev(
                                t0 + rng.range(0, 12 * HOUR),
                                s,
                                mule,
                                rng.amount(600.0, 0.5),
                                &w.device,
                            ));
                        }
                    }
                    "PASS-THROUGH" => {
                        let mule = w.fresh(young(rng));
                        let collector = w.fresh(young(rng));
                        let (mut t, mut total) = (t0, 0);
                        let k = 1 + rng.below(3);
                        for s in w.persons(rng, k) {
                            let a = rng.amount(5_000.0, 0.5);
                            out.push(ev(t, s, mule, a, &w.device));
                            total += a;
                            t += rng.range(0, 30 * MIN);
                        }
                        let fwd = total * rng.range(85, 99) / 100;
                        out.push(ev(
                            t + rng.range(2 * MIN, 30 * MIN),
                            mule,
                            collector,
                            fwd,
                            &w.device,
                        ));
                    }
                    "FAN-OUT" => {
                        let src = w.fresh(t0 - rng.range(3, 60) * DAY);
                        for _ in 0..12 + rng.below(19) {
                            let to = if rng.f() < 0.5 {
                                w.fresh(young(rng))
                            } else {
                                w.person(rng)
                            };
                            out.push(ev(
                                t0 + rng.range(0, 6 * HOUR),
                                src,
                                to,
                                rng.range(100_000, 900_000),
                                &w.device,
                            ));
                        }
                    }
                    "SCATTER-GATHER" => {
                        let src = w.fresh(t0 - rng.range(3, 60) * DAY);
                        let sink = w.fresh(young(rng));
                        for _ in 0..8 + rng.below(8) {
                            let mid = w.fresh(young(rng));
                            let (t, a) = (t0 + rng.range(0, 2 * HOUR), rng.range(300_000, 900_000));
                            out.push(ev(t, src, mid, a, &w.device));
                            let t2 = t + rng.range(10 * MIN, 90 * MIN);
                            out.push(ev(t2, mid, sink, a * rng.range(90, 99) / 100, &w.device));
                        }
                    }
                    "SHARED-DEVICE" => {
                        let members: Vec<u32> =
                            (0..4 + rng.below(5)).map(|_| w.fresh(young(rng))).collect();
                        let dev = w.device[members[0] as usize];
                        for &m in &members {
                            w.device[m as usize] = dev;
                        }
                        for &m in &members {
                            for _ in 0..2 + rng.below(4) {
                                let to = if rng.f() < 0.3 {
                                    members[rng.below(members.len())]
                                } else {
                                    w.person(rng)
                                };
                                if to != m {
                                    out.push(ev(
                                        t0 + rng.range(0, 48 * HOUR),
                                        m,
                                        to,
                                        rng.amount(300.0, 0.6),
                                        &w.device,
                                    ));
                                }
                            }
                        }
                    }
                    "VELOCITY" => {
                        let acct = w.person(rng);
                        let payees: Vec<u32> =
                            (0..3 + rng.below(6)).map(|_| w.fresh(young(rng))).collect();
                        let span = rng.range(20 * MIN, 55 * MIN);
                        for _ in 0..25 + rng.below(36) {
                            let to = payees[rng.below(payees.len())];
                            out.push(ev(
                                t0 + rng.range(0, span),
                                acct,
                                to,
                                rng.amount(800.0, 0.6),
                                &w.device,
                            ));
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        self.inj.sort_by_key(|e| e.ts);
    }

    fn background_day(&mut self, d: u32) -> Vec<Ev> {
        let (rng, w) = (&mut self.rng, &self.world);
        let day0 = T0 + d as i64 * DAY;
        let lo = w.persons_lo();
        let friday = (d + 3) % 7 == 4; // T0 is Thursday (Mon = 0).
        let payday = d % 30 == 24;
        let mut v = Vec::with_capacity((w.n_bg - lo) * 2);
        let ev = |ts, from: u32, to: u32, amount, scheme, device: Option<u32>| Ev {
            ts,
            from,
            to,
            amount,
            scheme,
            device,
            label: None,
        };
        for p in 0..w.n_bg - lo {
            let me = (lo + p) as u32;
            let dev = Some(w.device[me as usize]);
            // Payroll: employers fan out to employees (hubs for big employers).
            let emp = w.employer[p] as usize;
            if (w.weekly[emp] && friday) || (!w.weekly[emp] && payday) {
                let amt = if w.weekly[emp] {
                    w.salary[p] * 10 / 43
                } else {
                    w.salary[p]
                };
                v.push(ev(
                    day0 + 6 * HOUR + rng.range(0, 30 * MIN),
                    emp as u32,
                    me,
                    amt,
                    Scheme::Bacs,
                    None,
                ));
            }
            // Card spend: Pareto merchant popularity -> heavy-tailed in-degree.
            for _ in 0..rng.poisson(1.4) {
                let m = (w.n_emp + rng.skewed(w.n_merch, 3.0)) as u32;
                v.push(ev(
                    day0 + rng.range(7 * HOUR, 23 * HOUR),
                    me,
                    m,
                    rng.amount(25.0, 1.0),
                    Scheme::Card,
                    dev,
                ));
            }
            // P2P within the social circle.
            for _ in 0..rng.poisson(0.25) {
                let to = w.friends[p][rng.below(8)];
                if to != me {
                    v.push(ev(
                        day0 + rng.range(7 * HOUR, 23 * HOUR),
                        me,
                        to,
                        rng.amount(40.0, 1.0),
                        Scheme::Fps,
                        dev,
                    ));
                }
            }
            // Monthly bill to one of the 20 biggest merchants (utilities / landlords).
            if d % 30 == (p % 28) as u32 {
                let m = (w.n_emp + p % 20.min(w.n_merch)) as u32;
                v.push(ev(
                    day0 + rng.range(5 * HOUR, 9 * HOUR),
                    me,
                    m,
                    rng.amount(800.0, 0.4),
                    Scheme::Bacs,
                    None,
                ));
            }
        }
        v
    }

    fn txn(&self, e: &Ev, seq: u64) -> Txn {
        Txn {
            tx_id: format!("s{seq:09}"),
            ts_ms: e.ts,
            amount_minor: e.amount,
            currency: "GBP".into(),
            debtor: format!("acc{:07}", e.from),
            creditor: format!("acc{:07}", e.to),
            scheme: e.scheme,
            device_id: e.device.map(|d| format!("dev{d:07}")),
            ip: None,
            debtor_customer_hash: None,
            creditor_customer_hash: None,
            creditor_account_opened_at_ms: Some(self.world.opened[e.to as usize]),
        }
    }
}

impl Iterator for Synth {
    type Item = (Txn, Option<Label>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(e) = self.buf.next() {
                self.seq += 1;
                let label = e.label.map(|(t, a)| Label {
                    typology: TYPOLOGIES[t as usize].into(),
                    attempt: Some(a as u64),
                });
                return Some((self.txn(&e, self.seq), label));
            }
            if self.day >= self.days {
                return None;
            }
            let mut v = self.background_day(self.day);
            self.day += 1;
            let end = if self.day == self.days {
                i64::MAX
            } else {
                T0 + self.day as i64 * DAY
            };
            while self.inj_pos < self.inj.len() && self.inj[self.inj_pos].ts < end {
                v.push(self.inj[self.inj_pos]);
                self.inj_pos += 1;
            }
            v.sort_by_key(|e| e.ts);
            self.buf = v.into_iter();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn deterministic() {
        let a: Vec<_> = Synth::new(500, 5, 7, 2).collect();
        let b: Vec<_> = Synth::new(500, 5, 7, 2).collect();
        assert_eq!(a, b);
        let c: Vec<_> = Synth::new(500, 5, 8, 2).collect();
        assert_ne!(a, c);
    }

    #[test]
    fn labels_cover_every_typology_and_stream_is_sorted() {
        let items: Vec<_> = Synth::new(1000, 7, 1, 3).collect();
        assert!(items.windows(2).all(|w| w[0].0.ts_ms <= w[1].0.ts_ms));
        assert!(items
            .iter()
            .all(|(t, _)| t.amount_minor > 0 && t.debtor != t.creditor));
        let mut attempts: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        for (_, l) in &items {
            if let Some(l) = l {
                attempts
                    .entry(l.typology.clone())
                    .or_default()
                    .insert(l.attempt.unwrap());
            }
        }
        for typ in TYPOLOGIES {
            assert_eq!(attempts.get(typ).map(|s| s.len()), Some(3), "{typ}");
        }
        let unlabelled = items.iter().filter(|(_, l)| l.is_none()).count();
        assert!(
            unlabelled > items.len() * 9 / 10,
            "background must dominate"
        );
    }
}
