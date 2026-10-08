import { useState, type FormEvent } from 'react'
import { Link, useNavigate, useParams } from 'react-router'
import { api } from '../api/client'
import type { AlertDetail as Alert, DispositionLabel, SarExportResponse } from '../api/types'
import { Pill, ScoreBar } from '../components'
import GraphView from '../GraphView'
import { errMsg, fmtTime, money, RULE_LABEL, useLoad } from '../util'

const ANALYST_KEY = 'aegis.analyst'

export default function AlertDetail() {
  const { id = '' } = useParams()
  const nav = useNavigate()
  const [hops, setHops] = useState(1)
  const [sel, setSel] = useState<number | null>(0)
  const alert = useLoad(() => api.alert(id), [id])
  const graph = useLoad(() => api.subgraph(id, hops), [id, hops])
  const [busy, setBusy] = useState(false)
  const [actionErr, setActionErr] = useState<string>()
  const [sarData, setSarData] = useState<SarExportResponse | null>(null)
  const [sarLoading, setSarLoading] = useState(false)

  const a = alert.data
  if (alert.error && !a) return <div className="page"><div className="error-banner">{alert.error}</div></div>
  if (!a) return <div className="page"><div className="empty-state">Loading alert…</div></div>

  const finding = sel != null ? a.findings[sel] : undefined

  async function escalate(a: Alert) {
    setBusy(true)
    setActionErr(undefined)
    try {
      const c = await api.createCase({ title: `${a.rules.map((r) => RULE_LABEL[r]).join(' + ')} — ${a.debtor} → ${a.creditor}`, alert_ids: [a.id] })
      nav(`/cases/${c.id}`)
    } catch (e) {
      setActionErr(errMsg(e))
      setBusy(false)
    }
  }

  async function loadSar() {
    setSarLoading(true)
    setActionErr(undefined)
    try {
      const res = await api.generateSar(id)
      setSarData(res)
    } catch (e) {
      setActionErr(errMsg(e))
    } finally {
      setSarLoading(false)
    }
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="crumbs"><Link to="/alerts">Alerts</Link> <span className="dim">/</span> <span className="mono">{a.tx_id}</span></div>
          <h1 className="page-title alert-title">
            <span className="mono">{a.debtor}</span> <span className="dim">→</span> <span className="mono">{a.creditor}</span>
            <span className="amount">{money(a.amount_minor, a.currency)}</span>
          </h1>
          <div className="meta-row">
            <ScoreBar score={a.score} />
            <Pill v={a.decision} />
            <Pill v={a.status} />
            {a.rules.map((r) => <span key={r} className="chip">{RULE_LABEL[r] ?? r}</span>)}
            <span className="page-subtitle">created {fmtTime(a.created_at)}</span>
          </div>
        </div>
        <div className="filter-group">
          <button className="sm" onClick={loadSar} disabled={sarLoading}>
            {sarLoading ? 'Generating SAR…' : '⚖ Generate SAR Narrative'}
          </button>
          {a.case_id
            ? <Link className="btn-link" to={`/cases/${a.case_id}`}>Open case →</Link>
            : <button className="primary" disabled={busy || a.status === 'closed'} onClick={() => escalate(a)}>Escalate to case</button>}
        </div>
      </div>
      {actionErr && <div className="error-banner">{actionErr}</div>}

      {sarData && <SarModal sar={sarData} onClose={() => setSarData(null)} />}

      <div className="split-view">
        <div className="col">
          <div className="section-bar">
            <span className="section-title">Subgraph</span>
            <label className="inline">
              hops
              <select value={hops} onChange={(e) => setHops(Number(e.target.value))}>
                {[1, 2, 3].map((h) => <option key={h} value={h}>{h}</option>)}
              </select>
            </label>
          </div>
          {graph.error && <div className="error-banner">{graph.error}</div>}
          {graph.data
            ? <GraphView subgraph={graph.data} currency={a.currency} highlight={finding?.path ?? []} focus={[a.debtor, a.creditor]} />
            : <div className="graph-wrap"><div className="graph-canvas empty-state">Loading subgraph…</div></div>}

          {finding && (
            <div className="table-wrap">
              <div className="table-caption">Path · {RULE_LABEL[finding.rule]} · {finding.path.length} edges</div>
              <table className="dense-table">
                <thead><tr><th>#</th><th>Kind</th><th>From</th><th>To</th><th className="tar">Amount</th><th>Time</th><th>Tx</th></tr></thead>
                <tbody>
                  {finding.path.map((e, i) => (
                    <tr key={i} className={e.tx_id === a.tx_id ? 'row-current' : ''}>
                      <td className="dim">{i + 1}</td>
                      <td>{e.kind}</td>
                      <td className="mono">{e.from}</td>
                      <td className="mono">{e.to}</td>
                      <td className="tar mono">{e.kind === 'payment' ? money(e.amount_minor, a.currency) : '—'}</td>
                      <td className="mono">{fmtTime(e.ts_ms)}</td>
                      <td className="mono dim">{e.tx_id}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          <div className="table-wrap">
            <div className="table-caption">Transaction</div>
            <table className="dense-table kv">
              <tbody>
                <KV k="tx_id" v={a.txn.tx_id} />
                <KV k="time" v={fmtTime(a.txn.ts_ms)} />
                <KV k="amount" v={`${money(a.txn.amount_minor, a.txn.currency)} (${a.txn.amount_minor} minor)`} />
                <KV k="scheme" v={a.txn.scheme} />
                <KV k="debtor" v={a.txn.debtor} />
                <KV k="creditor" v={a.txn.creditor} />
                <KV k="device_id" v={a.txn.device_id} />
                <KV k="ip" v={a.txn.ip} />
                <KV k="debtor_customer_hash" v={a.txn.debtor_customer_hash} />
                <KV k="creditor_customer_hash" v={a.txn.creditor_customer_hash} />
                <KV k="creditor_account_opened" v={a.txn.creditor_account_opened_at_ms != null ? fmtTime(a.txn.creditor_account_opened_at_ms) : null} />
              </tbody>
            </table>
          </div>
        </div>

        <div className="col">
          <div className="card">
            <div className="card-header">Findings <span className="dim">{a.findings.length}</span></div>
            <div className="findings">
              {a.findings.map((f, i) => (
                <button key={i} className={`finding ${sel === i ? 'selected' : ''}`} onClick={() => setSel(sel === i ? null : i)}>
                  <div className="finding-top">
                    <span className="chip">{RULE_LABEL[f.rule] ?? f.rule}</span>
                    <ScoreBar score={f.score} />
                  </div>
                  <div className="finding-reason">{f.reason}</div>
                  <div className="dim">{f.path.length} path edges {sel === i ? '· highlighted' : '· click to highlight'}</div>
                </button>
              ))}
            </div>
          </div>
          <DispositionCard alert={a} onDone={alert.reload} />
        </div>
      </div>
    </div>
  )
}

function SarModal({ sar, onClose }: { sar: SarExportResponse; onClose: () => void }) {
  const [tab, setTab] = useState<'narrative' | 'fincen' | 'goaml'>('narrative')

  const download = (content: string, filename: string, type: string) => {
    const blob = new Blob([content], { type })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal-card sar-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h3>Suspicious Activity Report Studio</h3>
          <button className="sm" onClick={onClose}>✕</button>
        </div>
        <div className="sar-tabs">
          <button className={tab === 'narrative' ? 'active' : ''} onClick={() => setTab('narrative')}>
            Narrative (Markdown)
          </button>
          <button className={tab === 'fincen' ? 'active' : ''} onClick={() => setTab('fincen')}>
            FinCEN BSAR XML
          </button>
          <button className={tab === 'goaml' ? 'active' : ''} onClick={() => setTab('goaml')}>
            UK FIU / goAML XML
          </button>
        </div>
        <div className="sar-content">
          {tab === 'narrative' && (
            <div className="sar-narrative-view">
              <pre>{sar.narrative.full_markdown}</pre>
            </div>
          )}
          {tab === 'fincen' && (
            <div className="sar-xml-view">
              <pre>{sar.fincen_xml}</pre>
            </div>
          )}
          {tab === 'goaml' && (
            <div className="sar-xml-view">
              <pre>{sar.goaml_xml}</pre>
            </div>
          )}
        </div>
        <div className="modal-actions">
          {tab === 'narrative' && (
            <button
              onClick={() =>
                download(sar.narrative.full_markdown, `SAR-${sar.alert_id.slice(0, 8)}.md`, 'text/markdown')
              }
            >
              Download Markdown
            </button>
          )}
          {tab === 'fincen' && (
            <button
              className="primary"
              onClick={() =>
                download(sar.fincen_xml, `FinCEN-BSAR-${sar.alert_id.slice(0, 8)}.xml`, 'application/xml')
              }
            >
              Export FinCEN XML
            </button>
          )}
          {tab === 'goaml' && (
            <button
              className="primary"
              onClick={() =>
                download(sar.goaml_xml, `goAML-${sar.alert_id.slice(0, 8)}.xml`, 'application/xml')
              }
            >
              Export goAML XML
            </button>
          )}
          <button onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  )
}

function KV({ k, v }: { k: string; v: string | null | undefined }) {
  return <tr><td className="kv-key">{k}</td><td className="mono">{v ?? <span className="dim">—</span>}</td></tr>
}

function DispositionCard({ alert, onDone }: { alert: Alert; onDone: () => void }) {
  const [analyst, setAnalyst] = useState(() => localStorage.getItem(ANALYST_KEY) ?? '')
  const [note, setNote] = useState('')
  const [label, setLabel] = useState<DispositionLabel>()
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const d = alert.disposition

  if (d)
    return (
      <div className="card">
        <div className="card-header">Disposition</div>
        <div className="meta-row">
          <span className={`pill ${d.label === 'true_positive' ? 'block' : 'allow'}`}>{d.label.replace('_', ' ')}</span>
          <span>by <b>{d.analyst}</b></span>
          <span className="dim">{fmtTime(d.created_at)}</span>
        </div>
        {d.note && <p className="note-body">{d.note}</p>}
      </div>
    )

  async function submit(e: FormEvent) {
    e.preventDefault()
    if (!label) return
    setBusy(true)
    setError(undefined)
    try {
      localStorage.setItem(ANALYST_KEY, analyst.trim())
      await api.dispose(alert.id, { label, analyst: analyst.trim(), ...(note.trim() ? { note: note.trim() } : {}) })
      onDone()
    } catch (err) {
      setError(errMsg(err))
    } finally {
      setBusy(false)
    }
  }

  return (
    <form className="card form" onSubmit={submit}>
      <div className="card-header">Disposition</div>
      <div className="seg">
        <button type="button" className={label === 'true_positive' ? 'on tp' : ''} onClick={() => setLabel('true_positive')}>True positive</button>
        <button type="button" className={label === 'false_positive' ? 'on fp' : ''} onClick={() => setLabel('false_positive')}>False positive</button>
      </div>
      <input type="text" placeholder="Analyst name" value={analyst} onChange={(e) => setAnalyst(e.target.value)} />
      <textarea placeholder="Note (optional)" rows={3} value={note} onChange={(e) => setNote(e.target.value)} />
      {error && <div className="error-banner">{error}</div>}
      <button className="primary" disabled={!label || !analyst.trim() || busy}>{busy ? 'Saving…' : 'Record disposition'}</button>
      <p className="dim small">Closes the alert. Dispositions are kept as ML training labels.</p>
    </form>
  )
}
