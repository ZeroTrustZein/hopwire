//! Txn file formats: CSV (serde columns of `Txn`) or JSONL (one `Txn` per line), plus labels CSV.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use aegis_core::Txn;

use crate::{Label, Res};

fn is_jsonl(p: &Path) -> bool {
    matches!(
        p.extension().and_then(|e| e.to_str()),
        Some("jsonl" | "json" | "ndjson")
    )
}

/// `txns.csv` -> `txns.labels.csv`
pub fn labels_path(p: &Path) -> PathBuf {
    p.with_extension("labels.csv")
}

/// Streams txns from CSV or JSONL. Bad rows are reported and skipped.
pub fn read_txns(p: &Path) -> Res<Box<dyn Iterator<Item = Txn>>> {
    let f = BufReader::new(File::open(p)?);
    Ok(if is_jsonl(p) {
        Box::new(f.lines().enumerate().filter_map(|(i, l)| {
            let l = l.ok()?;
            if l.trim().is_empty() {
                return None;
            }
            serde_json::from_str(&l)
                .map_err(|e| eprintln!("line {}: {e}", i + 1))
                .ok()
        }))
    } else {
        Box::new(
            csv::Reader::from_reader(f)
                .into_deserialize()
                .filter_map(|r: csv::Result<Txn>| r.map_err(|e| eprintln!("{e}")).ok()),
        )
    })
}

pub fn read_labels(p: &Path) -> Res<HashMap<String, Label>> {
    let mut out = HashMap::new();
    for r in csv::Reader::from_path(p)?.into_records() {
        let r = r?;
        let attempt = r.get(2).and_then(|a| a.parse().ok());
        out.insert(
            r[0].to_string(),
            Label {
                typology: r[1].to_string(),
                attempt,
            },
        );
    }
    Ok(out)
}

pub fn write_synth(items: impl Iterator<Item = (Txn, Option<Label>)>, out: &Path) -> Res<()> {
    let mut labels = csv::Writer::from_path(labels_path(out))?;
    labels.write_record(["tx_id", "typology", "attempt"])?;
    let mut n = 0u64;
    let mut write_label = |t: &Txn, l: Option<Label>| -> csv::Result<()> {
        if let Some(l) = l {
            let a = l.attempt.map(|a| a.to_string()).unwrap_or_default();
            labels.write_record([t.tx_id.as_str(), &l.typology, &a])?;
        }
        Ok(())
    };
    if is_jsonl(out) {
        let mut w = BufWriter::new(File::create(out)?);
        for (t, l) in items {
            serde_json::to_writer(&mut w, &t)?;
            w.write_all(b"\n")?;
            write_label(&t, l)?;
            n += 1;
        }
        w.flush()?;
    } else {
        let mut w = csv::Writer::from_path(out)?;
        for (t, l) in items {
            w.serialize(&t)?;
            write_label(&t, l)?;
            n += 1;
        }
        w.flush()?;
    }
    eprintln!(
        "wrote {n} txns to {} (+ {})",
        out.display(),
        labels_path(out).display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_and_jsonl_round_trip() {
        let items: Vec<_> = crate::synth::Synth::new(300, 3, 3, 1).collect();
        let dir = std::env::temp_dir().join(format!("aegis-bench-io-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["t.csv", "t.jsonl"] {
            let p = dir.join(name);
            write_synth(items.clone().into_iter(), &p).unwrap();
            let back: Vec<Txn> = read_txns(&p).unwrap().collect();
            assert_eq!(
                back,
                items.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(),
                "{name}"
            );
            let labels = read_labels(&labels_path(&p)).unwrap();
            assert_eq!(
                labels.len(),
                items.iter().filter(|(_, l)| l.is_some()).count()
            );
            for (t, l) in &items {
                assert_eq!(labels.get(&t.tx_id), l.as_ref());
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
