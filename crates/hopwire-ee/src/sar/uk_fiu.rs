//! UK Financial Intelligence Unit (UK FIU / NCA) & UNODC goAML XML Schema.

use serde::{Deserialize, Serialize};

fn xml_escape(s: &str) -> String {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoAmlParty {
    pub client_number: String,
    pub name: String,
    pub role: String,
    pub account_number: String,
    pub institution_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoAmlTransaction {
    pub tx_id: String,
    pub date: String,
    pub amount_minor: i64,
    pub currency: String,
    pub from_account: String,
    pub to_account: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UkFiuSarData {
    pub report_id: String,
    pub reporting_entity: String,
    pub reason_for_suspicion: String,
    pub action_taken: String,
    pub parties: Vec<GoAmlParty>,
    pub transactions: Vec<GoAmlTransaction>,
}

/// Generates UNODC goAML standard XML export.
pub fn generate_goaml_xml(data: &UkFiuSarData) -> String {
    let mut txs_xml = String::new();
    for t in &data.transactions {
        let amt = format!("{:.2}", t.amount_minor as f64 / 100.0);
        txs_xml.push_str(&format!(
            r#"    <transaction>
      <transactionnumber>{}</transactionnumber>
      <date_transaction>{}</date_transaction>
      <amount>{}</amount>
      <currency_code>{}</currency_code>
      <from_account>{}</from_account>
      <to_account>{}</to_account>
      <comments>{}</comments>
    </transaction>
"#,
            xml_escape(&t.tx_id),
            xml_escape(&t.date),
            amt,
            xml_escape(&t.currency),
            xml_escape(&t.from_account),
            xml_escape(&t.to_account),
            xml_escape(&t.description)
        ));
    }

    let mut parties_xml = String::new();
    for p in &data.parties {
        parties_xml.push_str(&format!(
            r#"    <party>
      <client_number>{}</client_number>
      <name>{}</name>
      <role>{}</role>
      <account_number>{}</account_number>
      <institution_name>{}</institution_name>
    </party>
"#,
            xml_escape(&p.client_number),
            xml_escape(&p.name),
            xml_escape(&p.role),
            xml_escape(&p.account_number),
            xml_escape(&p.institution_name)
        ));
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<report xmlns="http://www.unodc.org/goAML">
  <rentity_id>{}</rentity_id>
  <submission_code>SAR</submission_code>
  <report_code>STR</report_code>
  <submission_date>{}</submission_date>
  <currency_code_local>GBP</currency_code_local>
  <reason>
<![CDATA[
{}
]]>
  </reason>
  <action>{}</action>
  <parties>
{}  </parties>
  <transactions>
{}  </transactions>
</report>"#,
        xml_escape(&data.reporting_entity),
        chrono::Utc::now().to_rfc3339(),
        data.reason_for_suspicion,
        xml_escape(&data.action_taken),
        parties_xml,
        txs_xml
    )
}
