//! FinCEN Suspicious Activity Report (BSAR) XML Generator and narrative template.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinCenSubject {
    pub name: String,
    pub account_id: String,
    pub role: String, // Debtor, Creditor, Intermediary
    pub address: Option<String>,
    pub tax_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinCenSarData {
    pub filing_institution: String,
    pub bsar_id: String,
    pub narrative: String,
    pub suspicious_activity_from: String,
    pub suspicious_activity_to: String,
    pub total_amount_minor: i64,
    pub currency: String,
    pub subjects: Vec<FinCenSubject>,
    pub typologies: Vec<String>,
}

/// Generates FinCEN-compliant XML format string.
pub fn generate_fincen_xml(data: &FinCenSarData) -> String {
    let mut subjects_xml = String::new();
    for s in &data.subjects {
        subjects_xml.push_str(&format!(
            r#"      <Party>
        <ActivityPartyTypeCode>{}</ActivityPartyTypeCode>
        <PartyName>
          <RawPartyFullName>{}</RawPartyFullName>
        </PartyName>
        <Account>
          <AccountNumberText>{}</AccountNumberText>
        </Account>
      </Party>
"#,
            if s.role == "Debtor" { "33" } else { "34" },
            s.name,
            s.account_id
        ));
    }

    let typologies_xml = data
        .typologies
        .iter()
        .map(|t| format!("        <SuspiciousActivityClassificationCode>{t}</SuspiciousActivityClassificationCode>"))
        .collect::<Vec<_>>()
        .join("\n");

    let amount_decimal = format!("{:.2}", data.total_amount_minor as f64 / 100.0);

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<EFilingBatchXML xmlns="www.fincen.gov/bsar">
  <Activity>
    <ApprovalDateText>{}</ApprovalDateText>
    <ActivitySeqNum>{}</ActivitySeqNum>
    <TotalSuspiciousAmountText>{}</TotalSuspiciousAmountText>
    <CurrencyCodeText>{}</CurrencyCodeText>
    <SuspiciousActivity>
{}
    </SuspiciousActivity>
    <Parties>
{}    </Parties>
    <SuspiciousActivityNarrativeText>
<![CDATA[
{}
]]>
    </SuspiciousActivityNarrativeText>
  </Activity>
</EFilingBatchXML>"#,
        chrono::Utc::now().format("%Y%m%d"),
        data.bsar_id,
        amount_decimal,
        data.currency,
        typologies_xml,
        subjects_xml,
        data.narrative
    )
}
