# AegisGraph: Seed Investment Memorandum & Pitch Deck Outline

## 1. Company Overview
- **Company**: AegisGraph Technologies
- **One-Liner**: Real-time in-memory graph AML infrastructure for instant payment switches.
- **Category**: Financial Infrastructure / RegTech / Cyber Crime Defense
- **Stage**: Pre-Seed / Seed Launch
- **Target Raise**: $2.5M – $3.5M Seed Round

---

## 2. The Urgent Catalyst: Why Now?
Compliance software has historically operated in asynchronous overnight batches because payment rails took 2 to 3 business days to settle. Two simultaneous global legislative shifts have made overnight batch monitoring economically catastrophic:

1. **UK PSR APP Fraud Mandatory Reimbursement (October 2024 / 2025 Mandate)**:
   - Imposes a maximum £85,000 reimbursement per Authorised Push Payment (APP) scam claim.
   - **Crucial shift: Mandatory 50/50 liability split between sending and receiving institutions.**
   - Historically, receiving banks faced zero liability when stolen funds were channeled into mule accounts. Now, receiving institutions suffer direct, immediate balance sheet depletion. Receiving mule network interception is now a capital solvency defense.
2. **EU Instant Payments Regulation (IPR Mandate)**:
   - All Payment Service Providers (PSPs) in the Single Euro Payments Area (SEPA) must clear and settle euro transfers in **under 10 seconds, 24/7/365**.
   - Compliance engines have less than **50 milliseconds** to render pre-clearance risk decisions or face penalties up to 10% of annual net turnover.

---

## 3. The Technical Failure of Incumbents
- **Relational Databases (SQL / RDBMS)**: Finding a 4-hop money laundering cycle requires recursive self-joins across millions of rows ($O(d^k)$ complexity). Under peak clearing volumes, queries exhaust database connection pools and blow past 100ms SLAs.
- **Enterprise Graph Consultancies (Quantexa)**: Rely on heavy Spark/Hadoop batch architectures. Valued at $2.6B, but requires multi-month enterprise consulting deployments and cannot score in-flight payments synchronously.
- **Graph Databases (Neo4j, Memgraph)**: Suffer from the "supernode freeze"—when a search encounters a high-degree merchant, utility, or payroll account, branching factors explode, causing severe latency spikes. They also charge $25k–$100k+ in OEM runtime licensing per customer.
- **Closed SaaS Vendors (Sardine, Unit21)**: Require unmasked customer financial PII to leave the bank's Virtual Private Cloud (VPC), triggering severe data residency and security resistance from tier-1 bank architects.

---

## 4. The AegisGraph Solution & Moat
AegisGraph is a self-hostable, single-binary transaction monitoring engine built in pure Rust:

1. **Sub-Millisecond Pre-Clearance Latency**: Bounded bidirectional BFS with sliding-window degree ceilings ($D_{\text{hub}} = 500$) guarantees deterministic cycle and mule detection (<25µs p99 for rules, <110µs with full neural inference).
2. **Dual-Path Intelligence**: Combines deterministic graph traversals with an embedded Continuous-Time Dynamic Graph Network (TGN) evaluated in-process via ONNX.
3. **Counterparty LRU Memory**: Bounded in-memory counterparty tracking suppresses recurring commercial payroll runs, reducing false positives by over 80%.
4. **Zero-PII Data Sovereignty**: Deploys entirely within the customer’s private VPC or Kubernetes cluster via Docker Compose or Helm. Zero egress of transaction records.
5. **Examiner-Grade Auditability (SR 11-7)**: Cryptographically chained SHA-256 audit ledger, real-time Population Stability Index (PSI) drift telemetry, and automated FinCEN / goAML SAR narrative generation.

---

## 5. Verified Performance Benchmarks

| Metric | AegisGraph Performance | Industry Benchmark |
|---|---|---|
| **Single-Core Throughput** | **407,000 txns/sec** (in-memory rules) / **22,200 txns/sec** (with ONNX TGN) / **25,600 txns/sec** (Postgres end-to-end) | 2,000 – 5,000 txns/sec |
| **Scoring Latency (p99)** | **9.2 microseconds** (rules) / **103.6 microseconds** (rules + TGN ensemble) | 50 – 150 milliseconds |
| **Independent Validation (IBM AML HI-Small)** | **96.3% attempt recall** (was 42.1% rules-alone) across all 8 laundering typologies; **76.9% txn recall** | < 50% attempt recall |
| **Scale Footprint** | **1,000,000 accounts & 77M payments in 5.7 GB RAM** | Distributed multi-node cluster |
| **False-Positive Suppression** | **88.5% precision on primary synthetic** / **65.5% on unseen holdout** via LRU counterparty memory | < 10% precision in legacy rules |

---

## 6. Business Model & Pricing
AegisGraph monetizes via an open-core commercial software model:
- **Community Core (Source-Available BSL 1.1)**: In-memory graph engine, bounded BFS cycle traversals, sliding degree checks, local Cytoscape console.
- **Commercial Enterprise Edition (`ee/`)**: Automated FinCEN/goAML SAR filing generators, continuous TGN retrain pipelines, OIDC/RBAC identity federation, active-active high-availability clustering, and cross-institutional consortium risk.

### Pricing Tiers:
- **Tier 3 (Emerging FinTechs & EMIs)**: $35,000 – $60,000 ACV (platform fee + volume tiers).
- **Tier 2 (Mid-Market Banks & Scaled PSPs)**: $80,000 – $250,000 ACV.
- **Tier 1 (Global Clearers & Payment Switches)**: $350,000 – $1,000,000+ ACV.

---

## 7. Go-To-Market Progression
1. **Phase 1: 14-Day Zero-Risk Historical Replay Pilots**:
   - Outbound pipeline targeting 50 UK payment institutions and EMIs exposed to the £85k PSR liability.
   - Customers replay 30–60 days of historical transactions into an isolated VPC container using `aegis-bench replay`.
   - Clear conversion criterion: Prove interception of confirmed historic receiving mule accounts and 50%+ reduction in payroll false positives.
2. **Phase 2: Core Banking & Switch Integrations**:
   - Native integration into cloud payment switches (Form3, ClearBank) and Redpanda Connect pipelines via our ISO 20022 `pacs.008` streaming connector.
3. **Phase 3: Upmarket Tier-1 Expansion**:
   - Regulatory risk packages satisfying Federal Reserve SR 11-7 standards, enterprise SSO, and multi-region clustering.

---

## 8. Financial Projections (Seed to Series A)

| Operating Phase | Ending ARR | Paying Customers | Blended ACV | Gross Margin |
|---|---|---|---|---|
| **Phase 1: Bootstrap (Mo 0–6)** | $0 (Pilots) | 3 Design Partners | N/A | N/A |
| **Phase 2: Seed Traction (Mo 6–18)** | $650,000 | 12 Customers | $54,166 | 82% |
| **Phase 3: Early Expansion (Mo 18–30)** | $2,100,000 | 26 Customers | $80,769 | 85% |
| **Phase 4: Series A Scale (Mo 30–42)** | $5,200,000 | 48 Customers | $108,333 | 88% |

---

## 9. The Seed Ask
- **Raising**: $3,000,000 Seed Round.
- **Use of Funds**:
  - **Engineering (60%)**: 3 senior systems engineers (distributed Rust systems, high-throughput networking) and 1 applied graph ML researcher.
  - **Go-To-Market (25%)**: 2 enterprise fintech account executives focused on UK and European PSPs.
  - **Regulatory & Compliance (15%)**: Formal SOC 2 Type II certification, independent SR 11-7 model validation audits, and legal/licensing operations.
