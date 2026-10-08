#[cfg(test)]
#[cfg(feature = "onnx")]
mod tests {
    use aegis_core::ml::tgn::TgnScorer;
    use aegis_core::{MlScorer, Scheme, Txn};
    use std::path::Path;

    #[test]
    fn tgn_scorer_loads_and_evaluates() {
        let model_path = Path::new("../../ml/models/tgn_scoring.onnx");
        if !model_path.exists() {
            eprintln!("Skipping test: model not generated yet at {:?}", model_path);
            return;
        }

        let mut scorer = TgnScorer::load(model_path, 0.01, 1.0).expect("failed to load ONNX model");
        let t1 = Txn {
            tx_id: "tx-ml-1".into(),
            ts_ms: 1_700_000_000_000,
            amount_minor: 50_000_00,
            currency: "GBP".into(),
            debtor: "acc-mule-1".into(),
            creditor: "acc-mule-2".into(),
            scheme: Scheme::Fps,
            device_id: None,
            ip: None,
            debtor_customer_hash: None,
            creditor_customer_hash: None,
            creditor_account_opened_at_ms: None,
        };

        let finding = scorer.score(&t1);
        assert!(
            finding.is_some(),
            "TGN should evaluate finding above low threshold"
        );
        let f = finding.unwrap();
        assert_eq!(f.rule, aegis_core::Rule::Ml);
        assert!(f.score > 0.0 && f.score <= 1.0);
        assert!(f.reason.contains("TGN neural network"));
        assert_eq!(
            scorer.memory_len(),
            2,
            "Both accounts should have memory updated"
        );
    }
}
