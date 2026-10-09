import { api } from '../api/client'
import { useLoad } from '../util'

export default function Governance() {
  const drift = useLoad(() => api.drift(), [])

  const rep = drift.data

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="crumbs">Telemetry</div>
          <h1 className="page-title">Population Stability Index (PSI) & Score Distribution</h1>
          <p className="page-subtitle">
            Tracks distribution shifts between calibrated baseline and active score evaluations.
          </p>
        </div>
        <div>
          <button onClick={() => drift.reload()}>
            ↻ Recalculate PSI
          </button>
        </div>
      </div>

      {drift.error && <div className="error-banner">{drift.error}</div>}
      {!rep && !drift.error && <div className="empty-state">Loading model telemetry…</div>}

      {rep && (
        <>
          <div className="stats-grid">
            <div className="card stat-card">
              <span className="stat-label">Population Stability Index (PSI)</span>
              <span className="stat-val mono">{rep.total_psi.toFixed(4)}</span>
              <span className={`pill ${rep.status === 'green' ? 'allow' : rep.status === 'yellow' ? 'review' : 'block'}`}>
                {rep.status.toUpperCase()}
              </span>
            </div>

            <div className="card stat-card">
              <span className="stat-label">Stability Threshold</span>
              <span className="stat-desc">
                {rep.status === 'green' && 'PSI < 0.10: Distribution stable. Baseline matches current scoring.'}
                {rep.status === 'yellow' && '0.10 ≤ PSI < 0.25: Moderate shift. Slight population or transaction variance.'}
                {rep.status === 'red' && 'PSI ≥ 0.25: Significant divergence between baseline and evaluated stream.'}
              </span>
            </div>

            <div className="card stat-card">
              <span className="stat-label">Evaluation Population</span>
              <span className="stat-val mono">{rep.current_count.toLocaleString()}</span>
              <span className="dim">Baseline: {rep.baseline_count.toLocaleString()} observations</span>
            </div>
          </div>

          <div className="card" style={{ marginTop: '20px' }}>
            <div className="card-header">
              <span>Score Distribution Deciles ({rep.metric_name})</span>
              <span className="dim">10-Bin Population Stability Breakdown</span>
            </div>
            <div className="table-wrap">
              <table className="dense-table">
                <thead>
                  <tr>
                    <th>Score Bin</th>
                    <th className="tar">Baseline Share</th>
                    <th className="tar">Evaluation Share</th>
                    <th className="tar">Shift (Δ)</th>
                    <th className="tar">PSI Contribution</th>
                  </tr>
                </thead>
                <tbody>
                  {rep.bins.map((b, i) => {
                    const diff = b.current_pct - b.baseline_pct
                    return (
                      <tr key={i}>
                        <td className="mono">{b.bin_start.toFixed(1)} – {b.bin_end.toFixed(1)}</td>
                        <td className="tar mono">{(b.baseline_pct * 100).toFixed(2)}%</td>
                        <td className="tar mono">{(b.current_pct * 100).toFixed(2)}%</td>
                        <td className={`tar mono ${diff > 0.05 ? 'block' : diff < -0.05 ? 'dim' : ''}`}>
                          {diff >= 0 ? '+' : ''}{(diff * 100).toFixed(2)}%
                        </td>
                        <td className="tar mono">{b.psi_contribution.toFixed(4)}</td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          </div>

          <div className="card" style={{ marginTop: '20px' }}>
            <div className="card-header">
              <span>Statistical Reference & Methodology</span>
            </div>
            <div style={{ padding: '16px', display: 'flex', flexDirection: 'column', gap: '8px', fontSize: '12px', color: 'var(--text-muted)' }}>
              <p style={{ margin: 0 }}>
                <strong>Formula:</strong> <code>PSI = Σ (Actual% - Expected%) × ln(Actual% / Expected%)</code> across 10 calibrated score deciles.
              </p>
              <p style={{ margin: 0 }}>
                Deciles partition the [0.0, 1.0] confidence output range. A high PSI signals that recent transaction patterns differ fundamentally from the reference distribution, indicating shifting fraud tactics or seasonal volume shifts.
              </p>
            </div>
          </div>
        </>
      )}
    </div>
  )
}
