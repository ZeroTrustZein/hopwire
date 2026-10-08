import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router'
import { api } from '../api/client'
import type { AlertFilters, AlertStatus, Rule } from '../api/types'
import { AlertTable, Pager } from '../components'
import { RULES, RULE_LABEL, useLoad } from '../util'

const LIMIT = 25

export default function AlertQueue() {
  const [params, setParams] = useSearchParams()
  const status = (params.get('status') ?? '') as AlertStatus | ''
  const rule = (params.get('rule') ?? '') as Rule | ''
  const minScore = params.get('min_score') ?? ''
  const offset = Number(params.get('offset') ?? '0')
  const [poll, setPoll] = useState(false)

  const filters: AlertFilters = {
    status: status || undefined,
    rule: rule || undefined,
    min_score: minScore ? Number(minScore) : undefined,
    limit: LIMIT,
    offset,
  }

  const { data, error, loading, reload } = useLoad(() => api.alerts(filters), [status, rule, minScore, offset])

  useEffect(() => {
    if (!poll) return
    const t = setInterval(reload, 5000)
    return () => clearInterval(t)
  }, [poll, reload])

  function patch(p: Record<string, string | null>) {
    const next = new URLSearchParams(params)
    for (const [k, v] of Object.entries(p)) {
      if (v === null || v === '') next.delete(k)
      else next.set(k, v)
    }
    if (!('offset' in p)) next.delete('offset')
    setParams(next, { replace: true })
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1 className="page-title">Alert Queue</h1>
          <p className="page-subtitle">Pre-clearance verdicts flagged for analyst review or block.</p>
        </div>
        <div className="filter-group">
          <label className="inline">
            <input type="checkbox" checked={poll} onChange={(e) => setPoll(e.target.checked)} />
            Auto-refresh (5s)
          </label>
          <button className="sm" onClick={reload} disabled={loading}>{loading ? 'Refreshing…' : 'Refresh'}</button>
        </div>
      </div>

      <div className="filter-bar">
        <div className="filter-group">
          <select value={status} onChange={(e) => patch({ status: e.target.value })}>
            <option value="">All statuses</option>
            <option value="open">Open</option>
            <option value="escalated">Escalated</option>
            <option value="closed">Closed</option>
          </select>
          <select value={rule} onChange={(e) => patch({ rule: e.target.value })}>
            <option value="">All rules</option>
            {RULES.map((r) => <option key={r} value={r}>{RULE_LABEL[r]}</option>)}
          </select>
          <select value={minScore} onChange={(e) => patch({ min_score: e.target.value })}>
            <option value="">Any score</option>
            <option value="0.6">Score ≥ 0.60</option>
            <option value="0.8">Score ≥ 0.80</option>
            <option value="0.9">Score ≥ 0.90</option>
          </select>
          {(status || rule || minScore || offset > 0) && (
            <button className="sm" onClick={() => setParams(new URLSearchParams(), { replace: true })}>Reset</button>
          )}
        </div>
        {data && <Pager total={data.total} limit={LIMIT} offset={offset} onOffset={(o) => patch({ offset: o ? String(o) : null })} />}
      </div>

      {error && <div className="error-banner">{error}</div>}
      <AlertTable items={data?.items ?? []} />
    </div>
  )
}
