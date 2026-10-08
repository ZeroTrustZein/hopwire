#![allow(dead_code)]

use aegis_core::{Config, Rule, Txn, Verdict};

pub const MIN: i64 = 60_000;
pub const HOUR: i64 = 60 * MIN;
pub const DAY: i64 = 24 * HOUR;
/// Arbitrary epoch base so timestamps look realistic.
pub const T0: i64 = 1_700_000_000_000;

pub fn tx(id: &str, ts: i64, amount: i64, debtor: &str, creditor: &str) -> Txn {
    Txn {
        tx_id: id.into(),
        ts_ms: T0 + ts,
        amount_minor: amount,
        currency: "GBP".into(),
        debtor: debtor.into(),
        creditor: creditor.into(),
        scheme: Default::default(),
        device_id: None,
        ip: None,
        debtor_customer_hash: None,
        creditor_customer_hash: None,
        creditor_account_opened_at_ms: None,
    }
}

/// Config with only `rule` enabled.
pub fn only(rule: Rule) -> Config {
    let mut c = Config::default();
    c.cycle.enabled = rule == Rule::Cycle;
    c.fan_in.enabled = rule == Rule::MuleFanIn;
    c.pass_through.enabled = rule == Rule::PassThrough;
    c.fan_out.enabled = rule == Rule::FanOut;
    c.velocity.enabled = rule == Rule::Velocity;
    c.shared_id.enabled = rule == Rule::SharedIdentifier;
    c
}

pub fn has(v: &Verdict, rule: Rule) -> bool {
    v.findings.iter().any(|f| f.rule == rule)
}

pub fn finding(v: &Verdict, rule: Rule) -> &aegis_core::Finding {
    v.findings
        .iter()
        .find(|f| f.rule == rule)
        .unwrap_or_else(|| panic!("no {rule:?} finding in {v:#?}"))
}
