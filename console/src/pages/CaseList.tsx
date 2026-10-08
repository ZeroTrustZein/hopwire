import { useNavigate, useSearchParams } from 'react-router'
import { api } from '../api/client'
import type { CaseStatus } from '../api/types'
import { Pager, Pill } from '../components'
import { ago, fmtTime, useLoad } from '../util'

const LIMIT = 25

export default function CaseList() {
  const nav = useNavigate()
  const [params, setParams] = useSearchParams()
  const status = (params.get('status') ?? '') as CaseStatus | ''
  const offset = Number(params.get('offset') ?? '0')
  const { data, error } = useLoad(() => api.cases({ status: status || undefined, limit: LIMIT, offset }), [status, offset])

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Cases</h1>
          <p className="page-subtitle">Escalated alerts under investigation.</p>
        </div>
      </div>
      <div className="filter-bar">
        <select value={status} onChange={(e) => setParams(e.target.value ? { status: e.target.value } : {}, { replace: true })}>
          <option value="">All statuses</option>
          <option value="open">Open</option>
          <option value="in_review">In review</option>
          <option value="closed">Closed</option>
        </select>
        {data && <Pager total={data.total} limit={LIMIT} offset={offset} onOffset={(o) => {
          const next = new URLSearchParams(params)
          if (o) next.set('offset', String(o))
          else next.delete('offset')
          setParams(next, { replace: true })
        }} />}
      </div>
      {error && <div className="error-banner">{error}</div>}
      <div className="table-wrap">
        {data && !data.items.length ? <div className="empty-state">No cases.</div> : (
          <table className="dense-table">
            <thead><tr><th>Title</th><th>Status</th><th>Assignee</th><th className="tar">Alerts</th><th>Created</th><th>Updated</th></tr></thead>
            <tbody>
              {data?.items.map((c) => (
                <tr key={c.id} className="clickable" onClick={() => nav(`/cases/${c.id}`)}>
                  <td>{c.title}</td>
                  <td><Pill v={c.status} /></td>
                  <td>{c.assignee ?? <span className="dim">unassigned</span>}</td>
                  <td className="tar mono">{c.alert_ids.length}</td>
                  <td>{fmtTime(c.created_at)}</td>
                  <td title={fmtTime(c.updated_at)}>{ago(c.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
