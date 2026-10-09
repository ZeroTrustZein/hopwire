# AegisGraph Design Partner Evaluation Agreement (14-Day Pilot)

This Evaluation Agreement ("Agreement") is entered into as of _________________ ("Effective Date"), by and between:

**Provider**: AegisGraph Technologies ("AegisGraph")  
**Customer**: _____________________________________________ ("Company")

---

### 1. Purpose and Scope of Evaluation
Company desires to evaluate the AegisGraph real-time transaction monitoring and graph anti-money laundering platform ("Software") to assess its performance, latency, and fraud detection efficacy.

1.1 **Evaluation Term**: The evaluation shall commence on the date the Software is deployed in Company’s environment and shall continue for fourteen (14) calendar days ("Evaluation Period").
1.2 **License Grant**: AegisGraph grants Company a non-exclusive, non-transferable, revocable license to deploy and execute the Software solely for internal testing and historical log replay evaluation.
1.3 **Zero Software Fee**: The Software is provided free of charge during the Evaluation Period.

---

### 2. Deployment Architecture & Data Sovereignty Warranty
2.1 **In-VPC Self-Hosted Execution**: The Software is containerized and shall be deployed entirely within Company’s private infrastructure (AWS VPC, GCP project, or on-premise bare-metal).
2.2 **Zero Data Egress Warranty**: AegisGraph expressly covenants and warrants that:
- No financial transaction data, account numbers, or personally identifiable information (PII) shall transit outside Company’s network boundary.
- The Software does not transmit telemetry containing customer identities or transaction amounts to external endpoints.
- Any customer identity hashes (e.g. SHA-256 tokens) utilized for entity resolution are computed and stored exclusively within Company’s dedicated instance.

---

### 3. Pilot Execution Protocol
3.1 **Historical Log Replay**: Company may replay up to ninety (90) days of sanitized or historical transaction logs through the AegisGraph replay harness (`aegis-bench replay`) into the isolated evaluation environment.
3.2 **Technical Support**: AegisGraph shall assign a dedicated Systems Engineer to assist Company during configuration and provide a one-hour response time SLA for critical technical inquiries during the Evaluation Period.

---

### 4. Mutual Success Criteria
The evaluation shall be deemed successful upon satisfying the following agreed benchmarks during the Evaluation Period:
1. **Detection Efficacy**: AegisGraph identifies at least seventy percent (70%) of Company's historically confirmed Authorised Push Payment (APP) fraud receiving mule accounts or multi-hop layering rings within the replay window.
2. **False-Positive Suppression**: In-memory counterparty memory demonstrates a reduction of at least fifty percent (50%) in false-positive alert volume on recurring commercial payroll runs relative to Company’s incumbent rules engine.
3. **Latency Profile**: Ninety-ninth percentile (p99) engine scoring latency remains below fifty (50) milliseconds under peak replay throughput.
4. **Audit Defensibility**: Cryptographic SHA-256 hash-chain verification (`/v1/audit/verify`) completes with zero integrity breaks.

---

### 5. Intellectual Property & Confidentiality
5.1 **Proprietary Rights**: AegisGraph retains all right, title, and interest in and to the Software, source code, underlying algorithms, and documentation.
5.2 **Company Data Ownership**: Company retains all right, title, and interest in all data, logs, and information provided or processed during the evaluation.
5.3 **Mutual Confidentiality**: Both parties agree to protect proprietary technical and commercial information with the same degree of care as their own confidential assets.

---

### 6. Post-Evaluation Commercial Terms
Upon successful completion of the Success Criteria set forth in Section 4:
6.1 Company shall have the option to transition to an Annual Commercial Enterprise License with a preferred Founding Design Partner discount of thirty percent (30%) applied to standard annual tier pricing.
6.2 If Company elects not to convert, Company shall destroy all local deployment containers and configuration keys within five (5) business days following the conclusion of the Evaluation Period.

---

### Signatures

**For AegisGraph Technologies**:  
Signature: ________________________________  
Name: _____________________________________  
Title: ______________________________________  
Date: ______________________________________  

**For Company**:  
Signature: ________________________________  
Name: _____________________________________  
Title: ______________________________________  
Date: ______________________________________  
