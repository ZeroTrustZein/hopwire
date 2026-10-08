//! Core SAR Narrative Synthesizer.
//!
//! Takes detected finding paths, typologies, and account topologies, and
//! constructs an examiner-grade natural language narrative following
//! FinCEN Section 5 and UK NCA SAR narrative guidelines.

pub mod fincen;
pub mod uk_fiu;

use aegis_core::{Finding, Rule, Txn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSarNarrative {
    pub title: String,
    pub executive_summary: String,
    pub suspect_breakdown: String,
    pub chronological_flow: String,
    pub topological_analysis: String,
    pub regulatory_nexus: String,
    pub recommended_action: String,
    pub full_markdown: String,
}

pub fn synthesize_narrative(
    txn: &Txn,
    findings: &[Finding],
    reporting_institution: &str,
) -> GeneratedSarNarrative {
    let top_rule = findings.first().map(|f| f.rule).unwrap_or(Rule::Cycle);
    let top_score = findings.first().map(|f| f.score).unwrap_or(0.85);

    let typology_label = match top_rule {
        Rule::Cycle => "Cyclic Layering & Circular Fund Routing",
        Rule::MuleFanIn => "Mule Inflow Aggregation & APP Fraud Collection",
        Rule::PassThrough => "Rapid Pass-Through & Immediate Mule Conduit Dissipation",
        Rule::FanOut => "Dispersal / Structuring across Distributed Payees",
        Rule::Velocity => "High-Frequency Velocity Outflow Burst",
        Rule::SharedIdentifier => "Synthetic Identity Ring & Shared Hardware Mule Syndicate",
        Rule::Ml => "Continuous-Time Temporal Graph Structural Anomaly",
    };

    let title = format!(
        "SUSPICIOUS ACTIVITY REPORT: {} on Account {}",
        typology_label.to_uppercase(),
        txn.creditor
    );

    let formatted_ts = chrono::DateTime::from_timestamp_millis(txn.ts_ms)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "recent date".into());
    let formatted_amt = format!("{:.2}", txn.amount_minor as f64 / 100.0);

    let executive_summary = format!(
        "{} has identified anomalous transaction activity indicative of {} on account {}. \
        On {}, transaction {} in the amount of {} {} was processed, triggering real-time graph \
        topology monitoring with an aggregate risk score of {:.2}/1.00.",
        reporting_institution,
        typology_label,
        txn.creditor,
        formatted_ts,
        txn.tx_id,
        formatted_amt,
        txn.currency,
        top_score
    );

    let suspect_breakdown = format!(
        "- Originating Account / Debtor: {}\n\
        - Receiving Account / Creditor: {}\n\
        - Payment Rail / Scheme: {:?}\n\
        - Associated Device Fingerprint: {}\n\
        - Associated IP Address: {}",
        txn.debtor,
        txn.creditor,
        txn.scheme,
        txn.device_id.as_deref().unwrap_or("None detected"),
        txn.ip.as_deref().unwrap_or("None detected")
    );

    let mut flow_rows = Vec::new();
    flow_rows.push(
        "| Leg | Kind | From | To | Amount | Transaction ID |\n|---|---|---|---|---|---|"
            .to_string(),
    );
    let mut leg_num = 1;
    for f in findings {
        for e in &f.path {
            let leg_amt = format!("{:.2} {}", e.amount_minor as f64 / 100.0, txn.currency);
            flow_rows.push(format!(
                "| {} | {:?} | {} | {} | {} | {} |",
                leg_num, e.kind, e.from, e.to, leg_amt, e.tx_id
            ));
            leg_num += 1;
        }
    }
    let chronological_flow = flow_rows.join("\n");

    let topological_analysis = findings
        .iter()
        .map(|f| {
            format!(
                "- **{:?} Rule (Confidence {:.1}%)**: {}",
                f.rule,
                f.score * 100.0,
                f.reason
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let regulatory_nexus = match top_rule {
        Rule::MuleFanIn | Rule::PassThrough => {
            "The observed rapid aggregation followed by immediate conduit dispersal matches \
            hallmarks of Authorised Push Payment (APP) fraud mule networks. Under UK PSR Mandatory \
            Reimbursement requirements and EU Instant Payments Regulations, receiving accounts exhibiting \
            unaccounted velocity without economic purpose represent severe balance sheet liability and \
            organized crime money laundering syndicates."
        }
        Rule::Cycle => {
            "The transactional topology exhibits complete circular routing where capital returns \
            to or near the originating entity. Such circular layering serves no lawful commercial utility \
            and is designed to obscure original criminal provenance across multi-hop interbank payment corridors."
        }
        _ => {
            "The velocity and network clustering characteristics violate baseline customer transactional profiles, \
            exhibiting characteristics consistent with money laundering smurfing syndicates."
        }
    }.to_string();

    let recommended_action = format!(
        "1. Temporarily freeze outgoing settlement privileges on account {}.\n\
        2. Submit formal Suspicious Activity Report (SAR / STR) filing to the national Financial Intelligence Unit.\n\
        3. Request source-of-funds documentation for counterparties across the identified graph path.\n\
        4. Preserve transaction lineage for law enforcement / FIU inspection.",
        txn.creditor
    );

    let full_markdown = format!(
        r#"# {}

## 1. Executive Summary
{}

## 2. Subject & Account Breakdown
{}

## 3. Chronological Financial Flow
{}

## 4. Graph Network & Topological Analysis
{}

## 5. Regulatory Nexus & Risk Characterization
{}

## 6. Recommended Action & Disposition
{}
"#,
        title,
        executive_summary,
        suspect_breakdown,
        chronological_flow,
        topological_analysis,
        regulatory_nexus,
        recommended_action
    );

    GeneratedSarNarrative {
        title,
        executive_summary,
        suspect_breakdown,
        chronological_flow,
        topological_analysis,
        regulatory_nexus,
        recommended_action,
        full_markdown,
    }
}
