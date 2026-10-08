import { useEffect, useRef, useState } from 'react'
import cytoscape, { type Core, type StylesheetJson } from 'cytoscape'
import type { NodeKind, PathEdge, Subgraph } from './api/types'
import { money } from './util'

const KIND: Record<NodeKind, { color: string; shape: string; label: string }> = {
  account: { color: '#3b82f6', shape: 'ellipse', label: 'Account' },
  device: { color: '#10b981', shape: 'round-rectangle', label: 'Device' },
  ip: { color: '#f59e0b', shape: 'diamond', label: 'IP' },
  customer: { color: '#ec4899', shape: 'hexagon', label: 'Customer' },
}
const PATH = '#ef4444'
const HUB = '#f97316'

export const edgeKey = (e: PathEdge) => `${e.kind}|${e.from}|${e.to}|${e.tx_id}`
const shortId = (id: string) => (id.length > 20 ? `${id.slice(0, 9)}…${id.slice(-6)}` : id)

function style(): StylesheetJson {
  const css = getComputedStyle(document.documentElement)
  const v = (n: string) => css.getPropertyValue(n).trim()
  return [
    {
      selector: 'node',
      style: {
        label: 'data(label)', color: v('--text-muted'), 'font-size': 9, 'font-family': v('--mono'),
        'text-valign': 'bottom', 'text-margin-y': 4, width: 22, height: 22,
        'background-color': 'data(color)', shape: 'data(shape)' as never,
        'border-width': 1, 'border-color': v('--surface'),
      },
    },
    { selector: 'node.focus', style: { width: 30, height: 30, 'border-width': 2, 'border-color': v('--text'), color: v('--text'), 'font-weight': 'bold' } },
    { selector: 'node.hub', style: { 'border-width': 4, 'border-color': HUB, 'border-style': 'double' } },
    { selector: 'node.hl', style: { 'border-width': 2, 'border-color': PATH } },
    {
      selector: 'edge',
      style: {
        width: 1.2, 'line-color': v('--border'), 'target-arrow-color': v('--border'), 'target-arrow-shape': 'triangle',
        'curve-style': 'bezier', 'arrow-scale': 0.8, label: 'data(label)', color: v('--text-dim'),
        'font-size': 8, 'font-family': v('--mono'), 'text-rotation': 'autorotate',
        'text-background-color': v('--surface'), 'text-background-opacity': 0.85, 'text-background-padding': '1px',
      },
    },
    { selector: 'edge.uses', style: { 'line-style': 'dashed', 'target-arrow-shape': 'none' } },
    {
      selector: 'edge.hl',
      style: { width: 3.5, 'line-color': PATH, 'target-arrow-color': PATH, 'z-index': 99, color: PATH, 'font-weight': 'bold', 'font-size': 9 },
    },
    { selector: '.faded', style: { opacity: 0.45 } },
  ]
}

export default function GraphView({ subgraph, currency, highlight, focus }: {
  subgraph: Subgraph
  currency: string
  /** Edges of the selected finding's path. */
  highlight: PathEdge[]
  /** Debtor / creditor of the alerted txn. */
  focus: string[]
}) {
  const ref = useRef<HTMLDivElement>(null)
  const cy = useRef<Core>(null)
  const [layout, setLayout] = useState<'cose' | 'breadthfirst' | 'concentric'>('cose')

  const focusSig = focus.join(',')

  // (Re)build graph when data changes.
  useEffect(() => {
    const focusIds = focusSig.split(',')
    const c = cytoscape({
      container: ref.current,
      style: style(),
      minZoom: 0.15,
      maxZoom: 4,
      elements: [
        ...subgraph.nodes.map((n) => ({
          data: { id: n.id, label: shortId(n.id), color: KIND[n.kind].color, shape: KIND[n.kind].shape },
          classes: [n.kind, n.hub && 'hub', focusIds.includes(n.id) && 'focus'].filter(Boolean).join(' '),
        })),
        ...subgraph.edges.map((e) => ({
          data: { id: edgeKey(e), source: e.from, target: e.to, label: e.kind === 'payment' ? money(e.amount_minor, currency, true) : '' },
          classes: e.kind,
        })),
      ],
    })
    cy.current = c
    const mq = matchMedia('(prefers-color-scheme: light)')
    const restyle = () => c.style(style())
    mq.addEventListener('change', restyle)
    return () => { mq.removeEventListener('change', restyle); c.destroy(); cy.current = null }
  }, [subgraph, currency, focusSig])

  // Highlight + layout.
  const hlSig = highlight.map(edgeKey).join(',')
  useEffect(() => {
    const c = cy.current
    if (!c) return
    const keys = new Set(hlSig.split(','))
    c.batch(() => {
      c.elements().removeClass('hl faded')
      if (!hlSig) return
      const hl = c.edges().filter((e) => keys.has(e.id()))
      hl.addClass('hl')
      hl.connectedNodes().addClass('hl')
      c.elements().not(hl).not(hl.connectedNodes()).addClass('faded')
    })
  }, [hlSig, subgraph])

  useEffect(() => {
    const c = cy.current
    if (!c) return
    const roots = c.nodes('.focus')
    c.layout({
      name: layout, animate: false, padding: 30, fit: true,
      ...(layout === 'cose' ? { idealEdgeLength: () => 70, nodeRepulsion: () => 9000, randomize: false } : {}),
      ...(layout === 'breadthfirst' ? { directed: true, roots: roots.length ? roots : undefined, spacingFactor: 1.1 } : {}),
      ...(layout === 'concentric' ? { concentric: (n: cytoscape.NodeSingular) => (n.hasClass('focus') ? 3 : n.hasClass('hl') ? 2 : 1), levelWidth: () => 1 } : {}),
      // Small graphs would otherwise be fit at huge zoom; cap it so labels stay readable.
      stop: () => { if (c.zoom() > 1.4) { c.zoom(1.4); c.center() } },
    } as cytoscape.LayoutOptions).run()
  }, [layout, subgraph, focusSig])

  const zoom = (f: number) => {
    const c = cy.current
    if (c) c.zoom({ level: c.zoom() * f, renderedPosition: { x: c.width() / 2, y: c.height() / 2 } })
  }
  const fit = () => {
    const c = cy.current
    if (!c) return
    const hl = c.elements('.hl')
    c.animate({ fit: { eles: hl.length ? hl : c.elements(), padding: 40 } }, { duration: 200 })
  }

  return (
    <div className="graph-wrap">
      <div className="graph-toolbar">
        <select value={layout} onChange={(e) => setLayout(e.target.value as typeof layout)} className="sm">
          <option value="cose">Force (cose)</option>
          <option value="breadthfirst">Flow (breadthfirst)</option>
          <option value="concentric">Concentric</option>
        </select>
        <button className="sm" onClick={() => zoom(1.25)} title="Zoom in">+</button>
        <button className="sm" onClick={() => zoom(0.8)} title="Zoom out">−</button>
        <button className="sm" onClick={fit} title="Fit highlighted path, or whole graph">Fit</button>
      </div>
      <div className="graph-legend">
        {(Object.keys(KIND) as NodeKind[]).map((k) => (
          <span key={k} className="legend-item"><span className={`legend-swatch ${KIND[k].shape}`} style={{ background: KIND[k].color }} />{KIND[k].label}</span>
        ))}
        <span className="legend-item"><span className="legend-swatch" style={{ border: `2px double ${HUB}` }} />Hub</span>
        <span className="legend-item"><span className="legend-line" style={{ background: PATH }} />Finding path</span>
        <span className="legend-item"><span className="legend-line dashed" />Uses</span>
        <span className="dim">{subgraph.nodes.length} nodes · {subgraph.edges.length} edges</span>
      </div>
      <div ref={ref} className="graph-canvas" />
    </div>
  )
}
