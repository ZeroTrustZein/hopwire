//! Loader for IBM "Realistic Synthetic Financial Transactions for AML" (Altman et al.).
//! Trans CSV: Timestamp,From Bank,Account,To Bank,Account,Amount Received,Receiving Currency,
//!            Amount Paid,Payment Currency,Payment Format,Is Laundering
//! Patterns:  BEGIN LAUNDERING ATTEMPT - <TYPE>[: details] / rows / END LAUNDERING ATTEMPT - <TYPE>

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader, Lines};
use std::path::Path;

use hopwire_core::{Scheme, Txn};

use crate::{Label, Res};

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub ts_ms: i64,
    pub from: String,
    pub to: String,
    pub amount_paid_minor: i64,
    pub currency: String,
    pub format: String,
    pub laundering: bool,
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2022/09/01 00:20` -> epoch ms (UTC).
pub fn parse_ts(s: &str) -> Option<i64> {
    let (date, time) = s.trim().split_once(' ')?;
    let mut d = date.split('/').map(|x| x.parse::<i64>());
    let (y, mo, da) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|x| x.parse::<i64>());
    let (h, mi) = (t.next()?.ok()?, t.next()?.ok()?);
    let sec = t.next().and_then(|x| x.ok()).unwrap_or(0);
    Some(((days_from_civil(y, mo, da) * 24 + h) * 60 + mi) * 60_000 + sec * 1000)
}

fn minor(s: &str) -> Option<i64> {
    Some((s.trim().parse::<f64>().ok()? * 100.0).round() as i64)
}

/// Parse one data row (the dataset has no quoted fields). Header / malformed rows -> None.
pub fn parse_row(line: &str) -> Option<Row> {
    let f: Vec<&str> = line.trim_end().split(',').collect();
    if f.len() < 11 {
        return None;
    }
    Some(Row {
        ts_ms: parse_ts(f[0])?,
        from: format!("{}-{}", f[1].trim(), f[2].trim()),
        to: format!("{}-{}", f[3].trim(), f[4].trim()),
        amount_paid_minor: minor(f[7])?,
        currency: f[8].trim().to_string(),
        format: f[9].trim().to_string(),
        laundering: f[10].trim() == "1",
    })
}

pub fn scheme(format: &str) -> Scheme {
    match format {
        "ACH" => Scheme::Bacs,
        "Wire" => Scheme::Chaps,
        "Credit Card" => Scheme::Card,
        _ => Scheme::Other,
    }
}

fn iso_currency(name: &str) -> &str {
    match name {
        "US Dollar" => "USD",
        "Euro" => "EUR",
        "UK Pound" => "GBP",
        "Yen" => "JPY",
        "Yuan" => "CNY",
        "Rupee" => "INR",
        "Ruble" => "RUB",
        "Canadian Dollar" => "CAD",
        "Australian Dollar" => "AUD",
        "Swiss Franc" => "CHF",
        "Mexican Peso" => "MXN",
        "Brazil Real" => "BRL",
        "Saudi Riyal" => "SAR",
        "Shekel" => "ILS",
        "Bitcoin" => "XBT",
        other => other,
    }
}

type Key = (String, String, i64, i64);

fn key(r: &Row) -> Key {
    (r.from.clone(), r.to.clone(), r.ts_ms, r.amount_paid_minor)
}

/// Patterns file: maps each attempt txn to (typology, attempt id).
#[derive(Debug, Default)]
pub struct Patterns {
    pub map: HashMap<Key, (String, u64)>,
    pub attempts: u64,
}

impl Patterns {
    pub fn load(p: &Path) -> Res<Self> {
        Ok(Self::parse(&std::fs::read_to_string(p)?))
    }

    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut cur: Option<String> = None;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("BEGIN LAUNDERING ATTEMPT - ") {
                let typ = rest.split(':').next().unwrap_or(rest).trim().to_string();
                cur = Some(typ);
                out.attempts += 1;
            } else if line.starts_with("END LAUNDERING ATTEMPT") {
                cur = None;
            } else if let (Some(typ), Some(row)) = (&cur, parse_row(line)) {
                out.map.insert(key(&row), (typ.clone(), out.attempts));
            }
        }
        out
    }

    pub fn label(&self, r: &Row) -> Option<Label> {
        match self.map.get(&key(r)) {
            Some((typ, a)) => Some(Label {
                typology: typ.clone(),
                attempt: Some(*a),
            }),
            None if r.laundering => Some(Label {
                typology: "UNATTRIBUTED".into(),
                attempt: None,
            }),
            None => None,
        }
    }
}

pub fn to_txn(r: Row, idx: u64) -> Txn {
    Txn {
        tx_id: format!("ibm-{idx}"),
        ts_ms: r.ts_ms,
        amount_minor: r.amount_paid_minor.max(1),
        currency: iso_currency(&r.currency).to_string(),
        scheme: scheme(&r.format),
        debtor: r.from,
        creditor: r.to,
        device_id: None,
        ip: None,
        debtor_customer_hash: None,
        creditor_customer_hash: None,
        creditor_account_opened_at_ms: None,
    }
}

struct Pending(i64, u64, Txn, Option<Label>);

impl PartialEq for Pending {
    fn eq(&self, o: &Self) -> bool {
        (self.0, self.1) == (o.0, o.1)
    }
}
impl Eq for Pending {}
impl PartialOrd for Pending {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Pending {
    // Reversed: BinaryHeap is a max-heap, we want earliest (ts, row) first.
    fn cmp(&self, o: &Self) -> Ordering {
        (o.0, o.1).cmp(&(self.0, self.1))
    }
}

/// Streams the Trans CSV in timestamp order using a bounded reorder heap.
pub struct IbmStream {
    lines: Lines<BufReader<File>>,
    pats: Patterns,
    heap: BinaryHeap<Pending>,
    cap: usize,
    row: u64,
    last_ts: i64,
    late: u64,
    skipped: u64,
}

impl IbmStream {
    pub fn open(p: &Path, pats: Patterns, cap: usize) -> Res<Self> {
        let lines = BufReader::with_capacity(1 << 20, File::open(p)?).lines();
        Ok(Self {
            lines,
            pats,
            heap: BinaryHeap::new(),
            cap: cap.max(1),
            row: 0,
            last_ts: i64::MIN,
            late: 0,
            skipped: 0,
        })
    }
}

impl Iterator for IbmStream {
    type Item = (Txn, Option<Label>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.heap.len() < self.cap {
            let Some(Ok(line)) = self.lines.next() else {
                break;
            };
            match parse_row(&line) {
                Some(r) => {
                    let label = self.pats.label(&r);
                    let ts = r.ts_ms;
                    self.heap
                        .push(Pending(ts, self.row, to_txn(r, self.row), label));
                    self.row += 1;
                }
                None if line.starts_with("Timestamp") => {}
                None => self.skipped += 1,
            }
        }
        let Some(Pending(ts, _, t, l)) = self.heap.pop() else {
            eprintln!(
                "ibm: {} rows, {} out-of-order after reorder buffer, {} malformed skipped",
                self.row, self.late, self.skipped
            );
            return None;
        };
        if ts < self.last_ts {
            self.late += 1;
        }
        self.last_ts = self.last_ts.max(ts);
        Some((t, l))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: &str = "2022/09/01 00:20,010,8000EBD30,010,8000EBD30,3697.34,US Dollar,3697.34,US Dollar,Reinvestment,0";

    #[test]
    fn parses_row() {
        let r = parse_row(ROW).unwrap();
        assert_eq!(r.from, "010-8000EBD30");
        assert_eq!(r.to, "010-8000EBD30");
        assert_eq!(r.amount_paid_minor, 369_734);
        assert_eq!(r.ts_ms, 1_661_991_600_000); // 2022-09-01T00:20:00Z
        assert!(!r.laundering);
        let t = to_txn(r, 7);
        assert_eq!(t.tx_id, "ibm-7");
        assert_eq!(t.currency, "USD");
        assert_eq!(t.scheme, Scheme::Other);
        assert!(parse_row("Timestamp,From Bank,Account,To Bank,Account,Amount Received,Receiving Currency,Amount Paid,Payment Currency,Payment Format,Is Laundering").is_none());
        assert_eq!(scheme("ACH"), Scheme::Bacs);
        assert_eq!(scheme("Wire"), Scheme::Chaps);
        assert_eq!(scheme("Credit Card"), Scheme::Card);
    }

    #[test]
    fn stream_sorts_by_timestamp() {
        let csv = "Timestamp,From Bank,Account,To Bank,Account,Amount Received,Receiving Currency,Amount Paid,Payment Currency,Payment Format,Is Laundering
2022/09/01 00:30,1,A,2,B,1.00,Euro,1.00,Euro,ACH,0
2022/09/01 00:10,1,A,2,B,2.00,Euro,2.00,Euro,ACH,1
2022/09/01 00:20,1,A,2,B,3.00,Euro,3.00,Euro,ACH,0
";
        let p = std::env::temp_dir().join(format!("hopwire-ibm-{}.csv", std::process::id()));
        std::fs::write(&p, csv).unwrap();
        let out: Vec<_> = IbmStream::open(&p, Patterns::default(), 10)
            .unwrap()
            .collect();
        std::fs::remove_file(&p).ok();
        let ids: Vec<&str> = out.iter().map(|(t, _)| t.tx_id.as_str()).collect();
        assert_eq!(ids, ["ibm-1", "ibm-2", "ibm-0"]);
        assert_eq!(out[0].1.as_ref().unwrap().typology, "UNATTRIBUTED");
    }

    #[test]
    fn parses_patterns() {
        let text = "\
BEGIN LAUNDERING ATTEMPT - FAN-OUT
2022/09/01 05:14,00952,8139F54E0,0111632,8062C56E0,5331.44,US Dollar,5331.44,US Dollar,ACH,1
2022/09/04 16:57,00952,8139F54E0,0118693,823D6D020,9073.75,US Dollar,9073.75,US Dollar,ACH,1
END LAUNDERING ATTEMPT - FAN-OUT

BEGIN LAUNDERING ATTEMPT - CYCLE:  Max 2 hops
2022/09/02 10:00,001,A,002,B,100.00,Euro,100.00,Euro,Wire,1
2022/09/02 11:00,002,B,001,A,95.00,Euro,95.00,Euro,Wire,1
END LAUNDERING ATTEMPT - CYCLE
";
        let p = Patterns::parse(text);
        assert_eq!(p.attempts, 2);
        assert_eq!(p.map.len(), 4);
        let r = parse_row("2022/09/02 11:00,002,B,001,A,95.00,Euro,95.00,Euro,Wire,1").unwrap();
        assert_eq!(
            p.label(&r),
            Some(Label {
                typology: "CYCLE".into(),
                attempt: Some(2)
            })
        );
        let r = parse_row("2022/09/04 16:57,00952,8139F54E0,0118693,823D6D020,9073.75,US Dollar,9073.75,US Dollar,ACH,1").unwrap();
        assert_eq!(p.label(&r).unwrap().typology, "FAN-OUT");
        let r = parse_row("2022/09/04 16:57,1,X,2,Y,1.00,Euro,1.00,Euro,Wire,1").unwrap();
        assert_eq!(p.label(&r).unwrap().typology, "UNATTRIBUTED");
        let r = parse_row(ROW).unwrap();
        assert_eq!(p.label(&r), None);
    }
}
