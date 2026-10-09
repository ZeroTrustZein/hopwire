// In-memory fixture backend for VITE_MOCK=1. Mirrors docs/api.md semantics (incl. mutations).
import { ApiError } from './client'
import type {
  AlertDetail, AlertSummary, Case, CaseDetail, Decision, Finding, Note, PathEdge, Rule, SubNode, Txn,
} from './types'

const NOW = Date.now()
const MIN = 60_000
const DAY = 24 * 60 * MIN
let seq = 0
// Deterministic UUID-shaped ids so URLs survive a page reload (fixtures are rebuilt identically).
let idSeq = 0
const uuid = () => `00000000-0000-4000-8000-${(++idSeq).toString(16).padStart(12, '0')}`
const iso = (ms: number) => new Date(ms).toISOString()
const pay = (from: string, to: string, amount_minor: number, ts_ms: number, tx_id = `tx-${(++seq).toString(36).padStart(5, '0')}`): PathEdge =>
  ({ kind: 'payment', from, to, tx_id, ts_ms, amount_minor })
const uses = (from: string, to: string, ts_ms: number, tx_id: string): PathEdge =>
  ({ kind: 'uses', from, to, tx_id, ts_ms, amount_minor: 0 })
const acct = (a: number, b: number) => `${String(a).padStart(6, '0').slice(-6)}-${String(b).padStart(8, '0').slice(-8)}`

interface Fixture { detail: AlertDetail; context: PathEdge[] }
const fixtures: Fixture[] = []
const HUBS = new Set(['ip:81.2.69.160', '000000-99000001'])

function add(o: {
  ago: number; score: number; decision: Decision; findings: Finding[]; trigger: PathEdge
  context?: PathEdge[]; txn?: Partial<Txn>; status?: AlertSummary['status']
}): AlertDetail {
  const t = o.trigger
  const txn: Txn = {
    tx_id: t.tx_id, ts_ms: t.ts_ms, amount_minor: t.amount_minor, currency: 'GBP',
    debtor: t.from, creditor: t.to, scheme: 'fps', device_id: null, ip: null,
    debtor_customer_hash: null, creditor_customer_hash: null, creditor_account_opened_at_ms: null, ...o.txn,
  }
  const detail: AlertDetail = {
    id: uuid(), tx_id: t.tx_id, created_at: iso(NOW - o.ago * MIN), ts_ms: t.ts_ms, score: o.score,
    decision: o.decision, rules: [...new Set(o.findings.map((f) => f.rule))].sort(),
    status: o.status ?? 'open', debtor: t.from, creditor: t.to, amount_minor: t.amount_minor,
    currency: txn.currency, case_id: null, findings: o.findings, txn, disposition: null,
  }
  fixtures.push({ detail, context: o.context ?? [] })
  return detail
}

// 1. Cycle: A -> B -> C -> A within 40 minutes, near-identical amounts.
{
  const [a, b, c] = ['204517-41829930', '309634-11873022', '601613-31926819']
  const t0 = NOW - 52 * MIN
  const e1 = pay(a, b, 980_000, t0), e2 = pay(b, c, 975_500, t0 + 14 * MIN), e3 = pay(c, a, 971_200, t0 + 38 * MIN)
  add({
    ago: 3, score: 0.91, decision: 'review', trigger: e3,
    findings: [
      { rule: 'cycle', score: 0.88, reason: 'Funds returned to originating account 204517-41829930 via 2 intermediaries within 38 min, losing 0.9% en route.', path: [e1, e2, e3] },
      { rule: 'pass_through', score: 0.64, reason: 'Account 309634-11873022 forwarded 99.5% of an inbound £9,800.00 within 14 min.', path: [e1, e2] },
    ],
    context: [pay('401122-77310045', a, 1_250_000, t0 - 300 * MIN), pay(c, '000000-99000001', 4_500, t0 - 90 * MIN), pay(b, '208811-55021934', 12_000, t0 - 200 * MIN)],
    txn: { device_id: 'a91f-22c0', ip: '86.14.201.7', creditor_account_opened_at_ms: NOW - 400 * DAY },
  })
}

// 2. Mule fan-in: 15 senders -> one young account, then cash-out.
const muleAlert = (() => {
  const mule = '772001-60451188'
  const t0 = NOW - 180 * MIN
  const ins = Array.from({ length: 15 }, (_, i) =>
    pay(acct(110000 + i * 3171, 20000000 + i * 7919337), mule, 45_000 + ((i * 37_123) % 180_000), t0 + i * 9 * MIN))
  const out = pay(mule, '090128-00347761', 1_480_000, t0 + 140 * MIN)
  return add({
    ago: 11, score: 0.97, decision: 'block', trigger: ins[14],
    findings: [
      { rule: 'mule_fan_in', score: 0.95, reason: '15 distinct senders paid 772001-60451188 (opened 6 days ago) in 2 h 6 min; none had prior contact.', path: ins },
      { rule: 'velocity', score: 0.71, reason: 'Inbound count 15 in 2 h vs. account baseline of 0.4/day.', path: ins.slice(-6) },
    ],
    context: [out, uses(mule, 'device:5c3e-d1a0', t0, ins[0].tx_id), uses(mule, 'ip:81.2.69.160', t0, ins[0].tx_id)],
    txn: { creditor_account_opened_at_ms: NOW - 6 * DAY, device_id: '5c3e-d1a0', ip: '81.2.69.160' },
    status: 'escalated',
  })
})()

// 3. Shared-device ring: 4 accounts / 4 customers on one device + IP.
{
  const accs = ['400515-12008876', '400515-12008914', '235018-90021455', '871244-30116620']
  const dev = 'device:ff09-7b21', ip = 'ip:92.40.188.23'
  const t0 = NOW - 300 * MIN
  const links = accs.flatMap((a, i) => [uses(a, dev, t0 + i * 20 * MIN, `tx-ring-${i}`), uses(a, ip, t0 + i * 20 * MIN, `tx-ring-${i}`)])
  const trig = pay(accs[3], '601613-88301447', 249_900, t0 + 70 * MIN, 'tx-ring-3')
  add({
    ago: 26, score: 0.72, decision: 'review', trigger: trig,
    findings: [{ rule: 'shared_identifier', score: 0.72, reason: 'Device ff09-7b21 and IP 92.40.188.23 shared by 4 accounts belonging to 4 different customers.', path: links }],
    context: [pay(accs[0], accs[2], 50_000, t0 + 5 * MIN), pay(accs[1], accs[3], 75_000, t0 + 30 * MIN), uses(accs[3], 'cust:77b0e913', t0, 'tx-ring-3')],
    txn: { device_id: 'ff09-7b21', ip: '92.40.188.23', debtor_customer_hash: 'c1e84fa2', creditor_customer_hash: '77b0e913' },
  })
}

// Filler: lower-signal alerts so pagination / filters have something to chew on.
const fillerRules: [Rule, string][] = [
  ['fan_out', 'Account dispersed {n} payments to new payees within 30 min.'],
  ['velocity', 'Outbound count {n} in 1 h vs. baseline of 1.1/day.'],
  ['pass_through', 'Account forwarded 9{n}% of an inbound credit within 20 min.'],
]
for (let i = 0; i < 34; i++) {
  const [rule, tmpl] = fillerRules[i % 3]
  const d = acct(300000 + i * 1931, 40000000 + i * 1237711)
  const c = acct(500000 + i * 2741, 60000000 + i * 3371917)
  const ts = NOW - (40 + i * 47) * MIN
  const path = [pay(d, c, 20_000 + ((i * 91_771) % 900_000), ts)]
  if (rule === 'fan_out') for (let k = 1; k <= 4; k++) path.push(pay(d, acct(510000 + i * 13 + k, 70000000 + k * 11), 15_000 + k * 3_300, ts - k * 4 * MIN))
  if (rule === 'pass_through') path.unshift(pay(acct(900000 + i, 10000000 + i * 7), d, path[0].amount_minor + 1_500, ts - 15 * MIN))
  const score = Math.round((0.35 + ((i * 0.137) % 0.55)) * 100) / 100
  const closed = i % 7 === 3
  const a = add({
    ago: 40 + i * 47, score, decision: score >= 0.6 ? 'review' : 'allow', trigger: path[path.length === 1 || rule === 'fan_out' ? 0 : path.length - 1],
    findings: [{ rule, score, reason: tmpl.replace('{n}', String(3 + (i % 6))), path }],
    status: closed ? 'closed' : 'open',
    txn: i % 5 === 2 ? { currency: 'EUR', scheme: 'other' } : i % 4 === 1 ? { scheme: 'chaps' } : {},
  })
  if (closed)
    a.disposition = { alert_id: a.id, label: 'false_positive', analyst: 'r.okafor', note: 'Payroll run, known pattern.', created_at: iso(NOW - (30 + i * 47) * MIN) }
}

// A case holding the mule alert.
const cases: CaseDetail[] = []
{
  const c: CaseDetail = {
    id: uuid(), title: 'Mule network — 772001-60451188 inbound cluster', status: 'in_review', assignee: 'j.mensah',
    created_at: iso(NOW - 9 * MIN), updated_at: iso(NOW - 4 * MIN), alert_ids: [muleAlert.id], alerts: [], notes: [],
  }
  muleAlert.case_id = c.id
  c.notes.push(
    { id: uuid(), case_id: c.id, author: 'j.mensah', body: 'Beneficiary opened 6 days ago via app; KYC selfie flagged low-confidence. Requested statements from receiving PSP.', created_at: iso(NOW - 8 * MIN) },
    { id: uuid(), case_id: c.id, author: 's.hale', body: 'Three of the senders reported APP scam to their banks this morning. Recommend SAR + freeze on outbound.', created_at: iso(NOW - 4 * MIN) },
  )
  cases.push(c)
}

const summary = ({ findings: _f, txn: _t, disposition: _d, ...s }: AlertDetail): AlertSummary => s
const caseOut = ({ alerts: _a, notes: _n, ...c }: CaseDetail): Case => c
const findAlert = (id: string) => {
  const f = fixtures.find((x) => x.detail.id === id)
  if (!f) throw new ApiError(404, 'alert not found')
  return f
}
const findCase = (id: string) => {
  const c = cases.find((x) => x.id === id)
  if (!c) throw new ApiError(404, 'case not found')
  return c
}
const page = <T,>(all: T[], q: URLSearchParams) => {
  const limit = Math.min(Number(q.get('limit') ?? 50), 500), offset = Number(q.get('offset') ?? 0)
  return { items: all.slice(offset, offset + limit), total: all.length }
}
const nodeKind = (id: string): SubNode['kind'] =>
  id.startsWith('device:') ? 'device' : id.startsWith('ip:') ? 'ip' : id.startsWith('cust:') ? 'customer' : 'account'

type Body = Record<string, unknown>

export async function mockRequest(method: string, fullPath: string, body: unknown): Promise<unknown> {
  await new Promise((r) => setTimeout(r, 120 + Math.random() * 180))
  const [path, query = ''] = fullPath.split('?')
  const q = new URLSearchParams(query)
  const b = (body ?? {}) as Body
  const seg = path.split('/').filter(Boolean).slice(1) // drop "v1"
  const route = `${method} /${seg.map((s, i) => (i === 1 && seg[0] !== 'audit' ? ':id' : s)).join('/')}`
  const id = seg[1]

  switch (route) {
    case 'GET /stats':
      return { nodes: 48_211, edges: 131_904, hubs: 12, processed: 1_204_377 + Math.floor((Date.now() - NOW) / 40), watermark_ms: Date.now() - 900 }
    case 'GET /audit/verify':
      return { ok: true, entries: 1_204_377 + fixtures.length * 2, broken_at: null }
    case 'GET /alerts': {
      const min = q.get('min_score'), st = q.get('status'), rule = q.get('rule') as Rule | null
      const all = fixtures.map((f) => f.detail)
        .filter((a) => (!st || a.status === st) && (!rule || a.rules.includes(rule)) && (!min || a.score >= Number(min)))
        .sort((x, y) => y.created_at.localeCompare(x.created_at))
      return page(all.map(summary), q)
    }
    case 'GET /alerts/:id':
      return findAlert(id).detail
    case 'GET /alerts/:id/subgraph': {
      const f = findAlert(id)
      const edges = [...new Map([...f.detail.findings.flatMap((x) => x.path), ...f.context]
        .map((e) => [`${e.kind}|${e.from}|${e.to}|${e.tx_id}`, e])).values()]
      const ids = new Set(edges.flatMap((e) => [e.from, e.to]))
      ids.add(f.detail.debtor).add(f.detail.creditor)
      return { nodes: [...ids].map((n) => ({ id: n, kind: nodeKind(n), hub: HUBS.has(n) })), edges }
    }
    case 'POST /alerts/:id/disposition': {
      const a = findAlert(id).detail
      if (b.label !== 'true_positive' && b.label !== 'false_positive') throw new ApiError(400, 'invalid label')
      if (!b.analyst) throw new ApiError(400, 'analyst required')
      a.disposition = { alert_id: id, label: b.label, analyst: String(b.analyst), note: (b.note as string) || null, created_at: iso(Date.now()) }
      a.status = 'closed'
      return a.disposition
    }
    case 'POST /alerts/:id/sar': {
      const a = findAlert(id).detail
      return {
        alert_id: id,
        narrative: {
          title: `SUSPICIOUS ACTIVITY REPORT: ${a.rules.join(', ').toUpperCase()} on ${a.creditor}`,
          executive_summary: `Hopwire identified anomalous multi-hop transaction activity on account ${a.creditor} with risk score ${a.score.toFixed(2)}.`,
          suspect_breakdown: `- Debtor: ${a.debtor}\n- Creditor: ${a.creditor}\n- Amount: £${(a.amount_minor / 100).toFixed(2)}`,
          chronological_flow: `| Leg | From | To | Amount |\n|---|---|---|---|\n| 1 | ${a.debtor} | ${a.creditor} | £${(a.amount_minor/100).toFixed(2)} |`,
          topological_analysis: a.findings.map(f => `- ${f.rule}: ${f.reason}`).join('\n'),
          regulatory_nexus: 'Observed network flow indicates structured layering and mule aggregation under UK PSR reimbursement and EU instant clearing mandates.',
          recommended_action: '1. Freeze account settlement privileges.\n2. File formal STR/SAR with UK FIU / FinCEN.',
          full_markdown: `# SUSPICIOUS ACTIVITY REPORT\n\n## Summary\nAlert on ${a.creditor}\n\n## Findings\n${a.findings.map(f => f.reason).join('\n')}`,
        },
        fincen_xml: '<?xml version="1.0" encoding="UTF-8"?><EFilingBatchXML xmlns="www.fincen.gov/bsar"><Activity><ActivitySeqNum>' + id + '</ActivitySeqNum></Activity></EFilingBatchXML>',
        goaml_xml: '<?xml version="1.0" encoding="UTF-8"?><report xmlns="http://www.unodc.org/goAML"><rentity_id>HOPWIRE</rentity_id></report>',
      }
    }
    case 'GET /model/drift': {
      return {
        metric_name: 'alert_risk_scores',
        total_psi: 0.042,
        status: 'green',
        baseline_count: 1000,
        current_count: 1000,
        bins: [
          { bin_start: 0.0, bin_end: 0.1, baseline_pct: 0.85, current_pct: 0.84, psi_contribution: 0.001 },
          { bin_start: 0.1, bin_end: 0.5, baseline_pct: 0.10, current_pct: 0.11, psi_contribution: 0.002 },
          { bin_start: 0.5, bin_end: 0.85, baseline_pct: 0.03, current_pct: 0.032, psi_contribution: 0.001 },
          { bin_start: 0.85, bin_end: 1.0, baseline_pct: 0.02, current_pct: 0.018, psi_contribution: 0.002 },
        ],
      }
    }
    case 'GET /cases': {
      const st = q.get('status')
      const all = cases.filter((c) => !st || c.status === st).sort((x, y) => y.created_at.localeCompare(x.created_at))
      return page(all.map(caseOut), q)
    }
    case 'POST /cases': {
      const alertIds = (b.alert_ids as string[]) ?? []
      const t = iso(Date.now())
      const c: CaseDetail = { id: uuid(), title: String(b.title ?? 'Untitled'), status: 'open', assignee: (b.assignee as string) ?? null, created_at: t, updated_at: t, alert_ids: alertIds, alerts: [], notes: [] }
      for (const aid of alertIds) Object.assign(findAlert(aid).detail, { status: 'escalated', case_id: c.id })
      cases.push(c)
      return caseOut(c)
    }
    case 'GET /cases/:id': {
      const c = findCase(id)
      return { ...c, alerts: c.alert_ids.map((a) => summary(findAlert(a).detail)) }
    }
    case 'PATCH /cases/:id': {
      const c = findCase(id)
      if (b.status !== undefined) c.status = b.status as Case['status']
      if (b.assignee !== undefined) c.assignee = (b.assignee as string) || null
      c.updated_at = iso(Date.now())
      return caseOut(c)
    }
    case 'POST /cases/:id/notes': {
      const c = findCase(id)
      const n: Note = { id: uuid(), case_id: id, author: String(b.author), body: String(b.body), created_at: iso(Date.now()) }
      c.notes.push(n)
      c.updated_at = n.created_at
      return n
    }
  }
  throw new ApiError(404, `mock: no route ${route}`)
}
