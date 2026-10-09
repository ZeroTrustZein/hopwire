# AegisGraph Outreach Campaign: High-Conversion Email Sequences

## Sequence 1: For Heads of Fraud & Financial Crime
**Target Persona**: Head of Fraud Operations, Financial Crime Director, ML Risk Lead  
**Pain Point**: UK Payment Systems Regulator (PSR) mandatory 50/50 reimbursement liability for receiving payment institutions on APP fraud (£85,000 threshold).

### Email 1: The PSR Reimbursement Reality (Initial Outreach)
**Subject**: Stopping receiving mule drain before settlement (sub-25µs graph check)  
**Preview**: The PSR 50/50 split means receiving accounts are now a direct balance sheet loss.

Hi {{FirstName}},

With the UK PSR 50/50 reimbursement rules in effect, receiving accounts are no longer just a routine AML reporting obligation—they represent immediate capital depletion when mule networks aren't intercepted before clearance.

Legacy tabular rules engines miss multi-hop structuring and pass-through conduits because they evaluate accounts in isolation. But traditional graph databases freeze or blow past your 100ms switch SLA the moment a transaction touches a high-volume merchant hub.

We built **AegisGraph**: a real-time, in-memory graph engine in pure Rust that evaluates incoming payments and detects multi-hop cycles, mule fan-ins, and rapid pass-throughs in **under 25 microseconds**.

On Altman et al.'s independent IBM AML benchmark, our engine demonstrated:
- **p99 latency under 40 microseconds** (155k–400k txns/sec single-core).
- **130x lift over the base laundering rate** at an ultra-low 0.07% alert rate.
- **Zero PII egress**: deploys 100% inside your private VPC/Kubernetes cluster via Docker or Helm.

We are offering selected UK payment institutions a **14-day evaluation**: replay a sanitized slice of historical payment logs into an isolated container in your VPC and measure exactly which receiving mule clusters your current stack missed.

Do you have 10 minutes next Tuesday or Wednesday to see the live graph replay console?

Best regards,  
[Your Name]  
Founder, AegisGraph  
[Link to Repository / Live Demo]

---

### Email 2: Follow-Up with Evidence (Day 4)
**Subject**: Re: Stopping receiving mule drain before settlement

Hi {{FirstName}},

Following up on my note regarding PSR receiving-mule liability.

Most risk teams we speak with cite two issues with graph AML:
1. **False-positive storms on payroll**: BACS runs to 50 employees triggering fan-out and velocity rules simultaneously.
2. **Examiner pushback on black-box ML**: Examiners rejecting unexplainable alert outputs under Model Risk guidance.

We solved both in the engine core:
- **Counterparty memory**: Bounded in-memory tracking of established payees suppresses recurring payroll and bills, cutting false positives by over 80% (65.5% precision on unseen holdout datasets).
- **Deterministic audit paths**: Every alert outputs the exact chronological payment subgraph and auto-generates a compliant FinCEN/goAML SAR narrative, proving why the alert fired to supervisory auditors.

If you’re open to it, I can share a 2-minute video walkthrough of the investigator console showing a 3-hop mule ring intercepted in real time.

Would that be helpful?

Best,  
[Your Name]

---

## Sequence 2: For CTOs, Chief Architects & Platform Engineers
**Target Persona**: CTO, Chief Architect, Head of Core Banking Platform  
**Pain Point**: Latency ceilings, database connection pool exhaustion from recursive self-joins, high Memgraph/Neo4j licensing and operational overhead.

### Email 1: Why RDBMS joins and general-purpose graphs fail at 10,000 TPS
**Subject**: Real-time graph AML without Memgraph OEM licensing or join latency

Hi {{FirstName}},

To detect a 4-hop money laundering cycle in a relational database, you need four recursive self-joins across millions of ledger rows: $O(d^k)$ complexity that exhausts connection pools during peak volume and blows past instant clearance SLAs.

Conversely, off-the-shelf graph databases (Neo4j, Memgraph) freeze on supernodes (utility aggregators, payroll hubs) and charge $25k–$100k+ in OEM runtime licensing per year.

We developed **AegisGraph**: an open-core in-memory graph AML substrate in Rust that runs directly inside your infrastructure:
- **Bounded BFS with Supernode degree ceilings**: Eliminates traversal freezes on high-degree merchant hubs while guaranteeing deterministic execution (<25µs p99).
- **Embedded pure-Rust Continuous-Time TGN model**: Evaluates neural temporal anomaly embeddings in-process via ONNX with zero Python sidecar latency.
- **Single binary footprint**: Scales to 1,000,000 accounts and 77,000,000 payments in under 5.7 GB RAM.
- **Native ISO 20022**: Direct ingestion of `pacs.008` XML with instant `pacs.002` payment status rejection generation.

The entire core is source-available under BSL 1.1 and compiles to a single container running in your VPC.

Would you be open to a technical deep dive or running our benchmark harness (`aegis-bench`) on your test infrastructure?

Best,  
[Your Name]  
Founder, AegisGraph

---

## Sequence 3: 14-Day PoC Offer Letter (Warm Lead Close)
**Subject**: 14-day AegisGraph Proof-of-Concept: Zero-risk log replay

Hi {{FirstName}},

Following our conversation, here is how we can validate AegisGraph on your actual payment data with zero production disruption:

**The 14-Day Evaluation Protocol**:
1. **Day 1–2**: We provide a containerized Docker Compose stack or Helm chart that you launch entirely inside your private VPC or sandbox environment. Zero financial data or PII ever leaves your boundary.
2. **Day 3–5**: Using our command-line replay tool (`aegis-bench replay`), you stream 30–60 days of historical transactions (JSON or ISO 20022 XML) into the engine.
3. **Day 6–10**: We evaluate detection metrics against your confirmed historic fraud cases:
   - Recall on confirmed receiving mule accounts.
   - Reduction in false-positive alert volume on corporate payroll.
   - Latency percentiles (p50, p90, p99) under your peak transaction volume.
4. **Day 11–14**: Review of generated SAR narratives and export audit reports with your compliance leads.

We provide this evaluation under a standard Mutual NDA and Evaluation Agreement with zero software fee during the pilot.

Shall we schedule 15 minutes to review the technical prerequisites and lock in next week's start date?

Best regards,  
[Your Name]
