//! ISO 20022 financial messaging parser and transformer for Hopwire.
//!
//! Provides zero-copy parsing for ISO 20022 `pacs.008` (Customer Credit Transfer)
//! into `hopwire_core::Txn`, and generation of `pacs.002` (Payment Status Report)
//! for real-time pre-clearance interception.

use chrono::{DateTime, NaiveDate, Utc};
use hopwire_core::{Decision, Scheme, Txn};
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum IsoError {
    #[error("XML parsing error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Invalid timestamp format: {0}")]
    InvalidTimestamp(String),
    #[error("Invalid amount format: {0}")]
    InvalidAmount(String),
    #[error("Missing mandatory element: {0}")]
    MissingField(&'static str),
}

/// Extracted credit transfer from a pacs.008 message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pacs008Txn {
    pub msg_id: String,
    pub cre_dt_tm: i64,
    pub end_to_end_id: Option<String>,
    pub uetr: Option<String>,
    pub tx_id: Option<String>,
    pub amount_minor: i64,
    pub currency: String,
    pub debtor_name: Option<String>,
    pub debtor_account: String,
    pub debtor_agent: Option<String>,
    pub creditor_name: Option<String>,
    pub creditor_account: String,
    pub creditor_agent: Option<String>,
    pub remittance_info: Option<String>,
}

impl Pacs008Txn {
    /// Convert parsed pacs.008 transfer into Hopwire core `Txn`.
    pub fn to_txn(&self) -> Txn {
        let tx_id = self
            .uetr
            .clone()
            .or_else(|| self.end_to_end_id.clone())
            .or_else(|| self.tx_id.clone())
            .unwrap_or_else(|| format!("{}-tx", self.msg_id));

        let scheme = match self.currency.to_uppercase().as_str() {
            "GBP" => Scheme::Fps,
            "EUR" => Scheme::Other, // SEPA / Instant
            _ => Scheme::Other,
        };

        let debtor_hash = self.debtor_name.as_ref().map(|nm| hex_sha256(nm.trim()));

        let creditor_hash = self.creditor_name.as_ref().map(|nm| hex_sha256(nm.trim()));

        Txn {
            tx_id,
            ts_ms: self.cre_dt_tm,
            amount_minor: self.amount_minor,
            currency: self.currency.clone(),
            debtor: self.debtor_account.clone(),
            creditor: self.creditor_account.clone(),
            scheme,
            device_id: None,
            ip: None,
            debtor_customer_hash: debtor_hash,
            creditor_customer_hash: creditor_hash,
            creditor_account_opened_at_ms: None,
        }
    }
}

fn hex_sha256(data: &str) -> String {
    use std::fmt::Write;
    let digest = <sha2_shim::Sha256 as sha2_shim::Digest>::digest(data.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        let _ = write!(s, "{:02x}", b);
    }
    s
}

mod sha2_shim {
    pub use sha2::{Digest, Sha256};
}

/// Parse ISO 20022 `pacs.008` XML string into a list of transactions.
pub fn parse_pacs008(xml: &str) -> Result<Vec<Pacs008Txn>, IsoError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut txns = Vec::new();
    let mut msg_id = String::new();
    let mut default_ts: i64 = Utc::now().timestamp_millis();

    let mut in_grp_hdr = false;
    let mut in_tx_inf = false;
    let mut in_dbtr = false;
    let mut in_dbtr_acct = false;
    let mut in_cdtr = false;
    let mut in_cdtr_acct = false;
    let mut in_dbtr_agt = false;
    let mut in_cdtr_agt = false;

    // Current tx builder
    let mut cur_e2e = None;
    let mut cur_uetr = None;
    let mut cur_tx_id = None;
    let mut cur_amount_minor = 0i64;
    let mut cur_currency = String::new();
    let mut cur_dbtr_nm = None;
    let mut cur_dbtr_acct = String::new();
    let mut cur_dbtr_agt = None;
    let mut cur_cdtr_nm = None;
    let mut cur_cdtr_acct = String::new();
    let mut cur_cdtr_agt = None;
    let mut cur_rmt_inf = None;
    let mut cur_tx_ts = None;

    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                current_tag = name.clone();
                match name.as_str() {
                    "GrpHdr" => in_grp_hdr = true,
                    "CdtTrfTxInf" => {
                        in_tx_inf = true;
                        cur_e2e = None;
                        cur_uetr = None;
                        cur_tx_id = None;
                        cur_amount_minor = 0;
                        cur_currency.clear();
                        cur_dbtr_nm = None;
                        cur_dbtr_acct.clear();
                        cur_dbtr_agt = None;
                        cur_cdtr_nm = None;
                        cur_cdtr_acct.clear();
                        cur_cdtr_agt = None;
                        cur_rmt_inf = None;
                        cur_tx_ts = None;
                    }
                    "Dbtr" => in_dbtr = true,
                    "DbtrAcct" => in_dbtr_acct = true,
                    "DbtrAgt" => in_dbtr_agt = true,
                    "Cdtr" => in_cdtr = true,
                    "CdtrAcct" => in_cdtr_acct = true,
                    "CdtrAgt" => in_cdtr_agt = true,
                    "IntrBkSttlmAmt" => {
                        for a in e.attributes().flatten() {
                            if a.key.local_name().as_ref() == b"Ccy" {
                                cur_currency = String::from_utf8_lossy(&a.value).to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(ref e) => {
                let txt = e.unescape()?.trim().to_string();
                if txt.is_empty() {
                    continue;
                }
                match current_tag.as_str() {
                    "MsgId" if in_grp_hdr => msg_id = txt,
                    "CreDtTm" => {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(&txt) {
                            let ms = dt.timestamp_millis();
                            if in_grp_hdr {
                                default_ts = ms;
                            }
                            if in_tx_inf {
                                cur_tx_ts = Some(ms);
                            }
                        }
                    }
                    "IntrBkSttlmDt" => {
                        if in_tx_inf && cur_tx_ts.is_none() {
                            if let Ok(d) = NaiveDate::parse_from_str(&txt, "%Y-%m-%d") {
                                cur_tx_ts = Some(
                                    d.and_hms_opt(12, 0, 0)
                                        .map(|dt| dt.and_utc().timestamp_millis())
                                        .unwrap_or(default_ts),
                                );
                            }
                        }
                    }
                    "EndToEndId" => cur_e2e = Some(txt),
                    "UETR" => cur_uetr = Some(txt),
                    "TxId" => cur_tx_id = Some(txt),
                    "IntrBkSttlmAmt" => {
                        cur_amount_minor = parse_amount_to_minor(&txt)?;
                    }
                    "Nm" if in_dbtr => cur_dbtr_nm = Some(txt),
                    "Nm" if in_cdtr => cur_cdtr_nm = Some(txt),
                    "IBAN" if in_dbtr_acct => cur_dbtr_acct = txt,
                    "IBAN" if in_cdtr_acct => cur_cdtr_acct = txt,
                    "Id" if in_dbtr_acct && cur_dbtr_acct.is_empty() => cur_dbtr_acct = txt,
                    "Id" if in_cdtr_acct && cur_cdtr_acct.is_empty() => cur_cdtr_acct = txt,
                    "BICFI" if in_dbtr_agt => cur_dbtr_agt = Some(txt),
                    "BICFI" if in_cdtr_agt => cur_cdtr_agt = Some(txt),
                    "MmbId" if in_dbtr_agt => cur_dbtr_agt = Some(txt),
                    "MmbId" if in_cdtr_agt => cur_cdtr_agt = Some(txt),
                    "Ustrd" => cur_rmt_inf = Some(txt),
                    _ => {}
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "GrpHdr" => in_grp_hdr = false,
                    "Dbtr" => in_dbtr = false,
                    "DbtrAcct" => in_dbtr_acct = false,
                    "DbtrAgt" => in_dbtr_agt = false,
                    "Cdtr" => in_cdtr = false,
                    "CdtrAcct" => in_cdtr_acct = false,
                    "CdtrAgt" => in_cdtr_agt = false,
                    "CdtTrfTxInf" => {
                        in_tx_inf = false;
                        if !cur_dbtr_acct.is_empty() && !cur_cdtr_acct.is_empty() {
                            let ts = cur_tx_ts.unwrap_or(default_ts);
                            let ccy = if cur_currency.is_empty() {
                                "GBP".to_string()
                            } else {
                                cur_currency.clone()
                            };
                            txns.push(Pacs008Txn {
                                msg_id: msg_id.clone(),
                                cre_dt_tm: ts,
                                end_to_end_id: cur_e2e.take(),
                                uetr: cur_uetr.take(),
                                tx_id: cur_tx_id.take(),
                                amount_minor: cur_amount_minor,
                                currency: ccy,
                                debtor_name: cur_dbtr_nm.take(),
                                debtor_account: cur_dbtr_acct.clone(),
                                debtor_agent: cur_dbtr_agt.take(),
                                creditor_name: cur_cdtr_nm.take(),
                                creditor_account: cur_cdtr_acct.clone(),
                                creditor_agent: cur_cdtr_agt.take(),
                                remittance_info: cur_rmt_inf.take(),
                            });
                        }
                    }
                    _ => {}
                }
                current_tag.clear();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(txns)
}

/// Parse decimal string e.g. "1250.75" into minor units 125075.
fn parse_amount_to_minor(s: &str) -> Result<i64, IsoError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(IsoError::InvalidAmount("empty string".into()));
    }
    let parts: Vec<&str> = s.split('.').collect();
    match parts.len() {
        1 => {
            let whole = parts[0]
                .parse::<i64>()
                .map_err(|_| IsoError::InvalidAmount(s.into()))?;
            Ok(whole * 100)
        }
        2 => {
            let whole = parts[0]
                .parse::<i64>()
                .map_err(|_| IsoError::InvalidAmount(s.into()))?;
            let frac_str = parts[1];
            let frac = match frac_str.len() {
                0 => 0i64,
                1 => {
                    frac_str
                        .parse::<i64>()
                        .map_err(|_| IsoError::InvalidAmount(s.into()))?
                        * 10
                }
                2 => frac_str
                    .parse::<i64>()
                    .map_err(|_| IsoError::InvalidAmount(s.into()))?,
                _ => {
                    // Round beyond 2 decimal places
                    let first_two = &frac_str[..2];
                    first_two
                        .parse::<i64>()
                        .map_err(|_| IsoError::InvalidAmount(s.into()))?
                }
            };
            Ok(whole * 100 + frac)
        }
        _ => Err(IsoError::InvalidAmount(s.into())),
    }
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Generate an ISO 20022 `pacs.002.001.10` Payment Status Report XML message
/// based on the Hopwire real-time pre-clearance decision.
pub fn generate_pacs002(
    orig_msg_id: &str,
    orig_tx_id: &str,
    orig_uetr: Option<&str>,
    decision: Decision,
    reason: &str,
) -> String {
    let (tx_status, rsn_code) = match decision {
        Decision::Allow => ("ACTC", "G000"), // Accepted Technical Validation
        Decision::Review => ("PDNG", "G001"), // Pending investigation
        Decision::Block => ("RJCT", "FRAD"), // Rejected - Fraudulent Origin
    };

    let now_utc = Utc::now().to_rfc3339();
    let resp_msg_id = format!(
        "HOPWIRE-{}-{}",
        now_utc.replace([':', '-'], ""),
        &orig_tx_id[..orig_tx_id.len().min(8)]
    );

    let esc_msg_id = xml_escape(orig_msg_id);
    let esc_tx_id = xml_escape(orig_tx_id);
    let esc_reason = xml_escape(reason);
    let uetr_elem = orig_uetr
        .map(|u| format!("<UETR>{}</UETR>", xml_escape(u)))
        .unwrap_or_default();

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.002.001.10">
  <FIToFIPmtStsRpt>
    <GrpHdr>
      <MsgId>{resp_msg_id}</MsgId>
      <CreDtTm>{now_utc}</CreDtTm>
    </GrpHdr>
    <TxInfAndSts>
      <OrgnlGrpInf>
        <OrgnlMsgId>{esc_msg_id}</OrgnlMsgId>
      </OrgnlGrpInf>
      <OrgnlEndToEndId>{esc_tx_id}</OrgnlEndToEndId>
      {uetr_elem}
      <TxSts>{tx_status}</TxSts>
      <StsRsnInf>
        <Rsn>
          <Cd>{rsn_code}</Cd>
        </Rsn>
        <AddtlInf>{esc_reason}</AddtlInf>
      </StsRsnInf>
    </TxInfAndSts>
  </FIToFIPmtStsRpt>
</Document>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_PACS008: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Document xmlns="urn:iso:std:iso:20022:tech:xsd:pacs.008.001.08">
  <FIToFICstmrCdtTrf>
    <GrpHdr>
      <MsgId>MSG20261008-001</MsgId>
      <CreDtTm>2026-10-08T14:30:00Z</CreDtTm>
      <NbOfTxs>1</NbOfTxs>
      <SttlmInf>
        <SttlmMtd>CLRG</SttlmMtd>
      </SttlmInf>
    </GrpHdr>
    <CdtTrfTxInf>
      <PmtId>
        <EndToEndId>E2E-2026-999</EndToEndId>
        <UETR>c0a80101-0000-0000-0000-000000000001</UETR>
        <TxId>TX-001</TxId>
      </PmtId>
      <IntrBkSttlmAmt Ccy="GBP">4500.50</IntrBkSttlmAmt>
      <IntrBkSttlmDt>2026-10-08</IntrBkSttlmDt>
      <Dbtr>
        <Nm>John Doe</Nm>
      </Dbtr>
      <DbtrAcct>
        <Id>GB29NWBK60161331926819</Id>
      </DbtrAcct>
      <DbtrAgt>
        <FinInstnId>
          <BICFI>NWBKGB2L</BICFI>
        </FinInstnId>
      </DbtrAgt>
      <CdtrAgt>
        <FinInstnId>
          <BICFI>BARCGB22</BICFI>
        </FinInstnId>
      </CdtrAgt>
      <Cdtr>
        <Nm>Acme Mules Ltd</Nm>
      </Cdtr>
      <CdtrAcct>
        <Id>GB33BARC20000012345678</Id>
      </CdtrAcct>
      <RmtInf>
        <Ustrd>Urgent payment invoice #104</Ustrd>
      </RmtInf>
    </CdtTrfTxInf>
  </FIToFICstmrCdtTrf>
</Document>"#;

    #[test]
    fn parse_pacs008_sample() {
        let txns = parse_pacs008(SAMPLE_PACS008).expect("parse failed");
        assert_eq!(txns.len(), 1);
        let t = &txns[0];
        assert_eq!(t.msg_id, "MSG20261008-001");
        assert_eq!(t.amount_minor, 450050);
        assert_eq!(t.currency, "GBP");
        assert_eq!(t.debtor_account, "GB29NWBK60161331926819");
        assert_eq!(t.debtor_name.as_deref(), Some("John Doe"));
        assert_eq!(t.creditor_account, "GB33BARC20000012345678");
        assert_eq!(t.creditor_name.as_deref(), Some("Acme Mules Ltd"));
        assert_eq!(
            t.uetr.as_deref(),
            Some("c0a80101-0000-0000-0000-000000000001")
        );

        let core_txn = t.to_txn();
        assert_eq!(core_txn.amount_minor, 450050);
        assert_eq!(core_txn.scheme, Scheme::Fps);
        assert!(core_txn.debtor_customer_hash.is_some());
        assert!(core_txn.creditor_customer_hash.is_some());
    }

    #[test]
    fn generate_pacs002_rejection() {
        let xml = generate_pacs002(
            "MSG20261008-001",
            "E2E-2026-999",
            Some("c0a80101-0000-0000-0000-000000000001"),
            Decision::Block,
            "Cycle detected: 3-hop mule ring",
        );
        assert!(xml.contains("<TxSts>RJCT</TxSts>"));
        assert!(xml.contains("<Cd>FRAD</Cd>"));
        assert!(xml.contains("<AddtlInf>Cycle detected: 3-hop mule ring</AddtlInf>"));
        assert!(xml.contains("<OrgnlMsgId>MSG20261008-001</OrgnlMsgId>"));
    }
}
