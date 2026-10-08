import { useNavigate } from 'react-router'
import type { AlertSummary } from './api/types'
import { ago, fmtTime, money, RULE_LABEL } from './util'

export function ScoreBar({ score }: { score: number }) {
  const lvl = score >= 0.85 ? 'high' : score >= 0.6 ? 'med' : 'low'
  return (
    <div className="score-bar-cell">
      <div className="score-track"><div className={`score-fill ${lvl}`} style={{ width: `${Math.round(score * 100)}%` }} /></div>
      <span className="mono">{score.toFixed(2)}</span>
    </div>
  )
}

export const Pill = ({ v }: { v: string }) => <span className={`pill ${v}`}>{v.replace('_', ' ')}</span>

export function AlertTable({ items }: { items: AlertSummary[] }) {
  const nav = useNavigate()
  if (!items.length) return <div className="table-wrap"><div className="empty-state">No alerts match.</div></div>
  return (
    <div className="table-wrap">
      <table className="dense-table">
        <thead>
          <tr>
            <th>Score</th><th>Decision</th><th>Rules</th><th>Debtor → Creditor</th>
            <th className="tar">Amount</th><th>Created</th><th>Status</th>
          </tr>
        </thead>
        <tbody>
          {items.map((a) => (
            <tr key={a.id} className="clickable" onClick={() => nav(`/alerts/${a.id}`)}>
              <td><ScoreBar score={a.score} /></td>
              <td><Pill v={a.decision} /></td>
              <td><div className="chips">{a.rules.map((r) => <span key={r} className="chip">{RULE_LABEL[r] ?? r}</span>)}</div></td>
              <td className="mono">{a.debtor} <span className="dim">→</span> {a.creditor}</td>
              <td className="tar mono">{money(a.amount_minor, a.currency)}</td>
              <td title={fmtTime(a.created_at)}>{ago(a.created_at)}</td>
              <td><Pill v={a.status} /></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export function Pager({ total, limit, offset, onOffset }: { total: number; limit: number; offset: number; onOffset: (o: number) => void }) {
  if (total <= limit && offset === 0) return <span className="page-subtitle">{total} total</span>
  return (
    <div className="filter-group">
      <span className="page-subtitle">{total ? offset + 1 : 0}–{Math.min(offset + limit, total)} of {total}</span>
      <button className="sm" disabled={offset === 0} onClick={() => onOffset(Math.max(0, offset - limit))}>Prev</button>
      <button className="sm" disabled={offset + limit >= total} onClick={() => onOffset(offset + limit)}>Next</button>
    </div>
  )
}
