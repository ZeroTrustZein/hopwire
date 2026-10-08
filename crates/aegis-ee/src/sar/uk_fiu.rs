//! UK Financial Intelligence Unit (UK FIU / NCA) & UNODC goAML XML Schema.

use serde::{Deserialize, Serialize};

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
            t.tx_id, t.date, amt, t.currency, t.from_account, t.to_account, t.description
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
            p.client_number, p.name, p.role, p.account_number, p.institution_name
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
        data.reporting_entity,
        chrono::Utc::now().to_rfc3339(),
        data.reason_for_suspicion,
        data.action_taken,
        parties_xml,
        txs_xml
    )
}
