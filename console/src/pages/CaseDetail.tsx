import { useEffect, useState, type FormEvent } from 'react'
import { Link, useParams } from 'react-router'
import { api } from '../api/client'
import type { CaseStatus } from '../api/types'
import { AlertTable, Pill } from '../components'
import { errMsg, fmtTime, useLoad } from '../util'

const ANALYST_KEY = 'aegis.analyst'

export default function CaseDetail() {
  const { id = '' } = useParams()
  const { data: c, error, reload } = useLoad(() => api.case(id), [id])
  const [status, setStatus] = useState<CaseStatus>('open')
  const [assignee, setAssignee] = useState('')
  const [author, setAuthor] = useState(() => localStorage.getItem(ANALYST_KEY) ?? '')
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)
  const [actionErr, setActionErr] = useState<string>()

  useEffect(() => {
    if (c) { setStatus(c.status); setAssignee(c.assignee ?? '') }
  }, [c])

  if (error && !c) return <div className="page"><div className="error-banner">{error}</div></div>
  if (!c) return <div className="page"><div className="empty-state">Loading case…</div></div>

  const dirty = status !== c.status || assignee.trim() !== (c.assignee ?? '')

  async function run(fn: () => Promise<unknown>) {
    setBusy(true)
    setActionErr(undefined)
    try { await fn(); reload(); return true } catch (e) { setActionErr(errMsg(e)); return false } finally { setBusy(false) }
  }

  const save = () => run(() => api.patchCase(id, {
    ...(status !== c.status ? { status } : {}),
    ...(assignee.trim() !== (c.assignee ?? '') ? { assignee: assignee.trim() } : {}),
  }))

  async function addNote(e: FormEvent) {
    e.preventDefault()
    localStorage.setItem(ANALYST_KEY, author.trim())
    if (await run(() => api.addNote(id, { author: author.trim(), body: body.trim() }))) setBody('')
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="crumbs"><Link to="/cases">Cases</Link> <span className="dim">/</span> <span className="mono">{c.id.slice(0, 8)}</span></div>
          <h1 className="page-title">{c.title}</h1>
          <div className="meta-row">
            <Pill v={c.status} />
            <span className="page-subtitle">opened {fmtTime(c.created_at)} · updated {fmtTime(c.updated_at)}</span>
          </div>
        </div>
        <div className="filter-group">
          <select value={status} onChange={(e) => setStatus(e.target.value as CaseStatus)}>
            <option value="open">Open</option>
            <option value="in_review">In review</option>
            <option value="closed">Closed</option>
          </select>
          <input type="text" placeholder="Assignee" value={assignee} onChange={(e) => setAssignee(e.target.value)} />
          <button className="primary" disabled={!dirty || busy} onClick={save}>Save</button>
        </div>
      </div>
      {actionErr && <div className="error-banner">{actionErr}</div>}

      <div className="split-view">
        <div className="col">
          <div className="section-bar"><span className="section-title">Linked alerts <span className="dim">{c.alerts.length}</span></span></div>
          <AlertTable items={c.alerts} />
        </div>
        <div className="col">
          <div className="card">
            <div className="card-header">Notes <span className="dim">{c.notes.length}</span></div>
            <div className="notes">
              {c.notes.length === 0 && <div className="dim">No notes yet.</div>}
              {c.notes.map((n) => (
                <div key={n.id} className="note">
                  <div className="note-head"><b>{n.author}</b><span className="dim">{fmtTime(n.created_at)}</span></div>
                  <p className="note-body">{n.body}</p>
                </div>
              ))}
            </div>
            <form className="form note-form" onSubmit={addNote}>
              <input type="text" placeholder="Your name" value={author} onChange={(e) => setAuthor(e.target.value)} />
              <textarea placeholder="Add a note…" rows={3} value={body} onChange={(e) => setBody(e.target.value)} />
              <button className="primary" disabled={!author.trim() || !body.trim() || busy}>Add note</button>
            </form>
          </div>
        </div>
      </div>
    </div>
  )
}
