import { lazy, Suspense, useEffect, useState } from 'react'
import { Navigate, NavLink, Route, Routes, useNavigate } from 'react-router'
import { api, auth, setUnauthorizedHandler } from './api/client'
import type { AuditVerify, EngineStats } from './api/types'
import { fmtInt } from './util'
import Login from './pages/Login'
import AlertQueue from './pages/AlertQueue'
import CaseList from './pages/CaseList'
import CaseDetail from './pages/CaseDetail'
import Governance from './pages/Governance'

// Cytoscape is the bulk of the bundle; load it only on the alert detail route.
const AlertDetail = lazy(() => import('./pages/AlertDetail'))

function Header({ onLogout }: { onLogout: () => void }) {
  const [stats, setStats] = useState<EngineStats>()
  const [audit, setAudit] = useState<AuditVerify | null>()
  useEffect(() => {
    let live = true
    // Audit verify recomputes the whole hash chain server-side: check once per session, poll only stats.
    api.auditVerify().then((a) => live && setAudit(a)).catch(() => live && setAudit(null))
    const tick = () => { api.stats().then((s) => live && setStats(s)).catch(() => {}) }
    tick()
    const t = setInterval(tick, 15_000)
    return () => { live = false; clearInterval(t) }
  }, [])
  return (
    <header className="app-header">
      <div className="header-left">
        <div className="logo"><span className="logo-icon" />Hopwire</div>
        <nav className="nav-links">
          <NavLink to="/alerts">Alerts</NavLink>
          <NavLink to="/cases">Cases</NavLink>
          <NavLink to="/governance">Model Drift</NavLink>
        </nav>
      </div>
      <div className="header-right">
        {stats && (
          <div className="stats-strip" title={`Watermark ${new Date(stats.watermark_ms).toISOString()}`}>
            <span className="stat-pill">nodes <b>{fmtInt(stats.nodes)}</b></span>
            <span className="stat-pill">edges <b>{fmtInt(stats.edges)}</b></span>
            <span className="stat-pill">hubs <b>{fmtInt(stats.hubs)}</b></span>
            <span className="stat-pill">processed <b>{fmtInt(stats.processed)}</b></span>
          </div>
        )}
        {audit !== undefined && (
          <span
            className={`audit-badge ${audit?.ok ? 'ok' : 'broken'}`}
            title={audit ? `${fmtInt(audit.entries)} entries${audit.broken_at != null ? `, broken at seq ${audit.broken_at}` : ''}` : 'Audit check failed'}
          >
            <span className="dot" />
            {audit == null ? 'audit unknown' : audit.ok ? 'audit chain ok' : `audit broken @${audit.broken_at}`}
          </span>
        )}
        <button className="sm" onClick={onLogout}>Sign out</button>
      </div>
    </header>
  )
}

export default function App() {
  const nav = useNavigate()
  const [authed, setAuthed] = useState(() => !!auth.get())
  useEffect(() => setUnauthorizedHandler(() => { setAuthed(false); nav('/login') }), [nav])

  if (!authed)
    return (
      <Routes>
        <Route path="*" element={<Login onLogin={() => { setAuthed(true); nav('/alerts') }} />} />
      </Routes>
    )

  return (
    <div className="app-shell">
      <Header onLogout={() => { auth.clear(); setAuthed(false); nav('/login') }} />
      <main className="app-main">
        <Suspense fallback={<div className="page"><div className="empty-state">Loading…</div></div>}>
        <Routes>
          <Route path="/alerts" element={<AlertQueue />} />
          <Route path="/alerts/:id" element={<AlertDetail />} />
          <Route path="/cases" element={<CaseList />} />
          <Route path="/cases/:id" element={<CaseDetail />} />
          <Route path="/governance" element={<Governance />} />
          <Route path="*" element={<Navigate to="/alerts" replace />} />
        </Routes>
        </Suspense>
      </main>
    </div>
  )
}
