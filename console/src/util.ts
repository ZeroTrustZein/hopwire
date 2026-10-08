import { useCallback, useEffect, useState } from 'react'
import type { Rule } from './api/types'

export const RULES: Rule[] = ['cycle', 'mule_fan_in', 'pass_through', 'fan_out', 'velocity', 'shared_identifier', 'ml']
export const RULE_LABEL: Record<Rule, string> = {
  cycle: 'Cycle', mule_fan_in: 'Mule fan-in', pass_through: 'Pass-through', fan_out: 'Fan-out',
  velocity: 'Velocity', shared_identifier: 'Shared identifier', ml: 'ML',
}

const fmtCache = new Map<string, Intl.NumberFormat>()
/** Minor units -> localized major amount, using the currency's own minor-unit digits (JPY 0, GBP 2, ...). */
export function money(minor: number, currency: string, compact = false): string {
  const k = currency + compact
  let f = fmtCache.get(k)
  if (!f) {
    try {
      f = new Intl.NumberFormat(undefined, { style: 'currency', currency, ...(compact ? { notation: 'compact', maximumFractionDigits: 1 } : {}) })
    } catch {
      f = new Intl.NumberFormat(undefined, { minimumFractionDigits: 2 })
    }
    fmtCache.set(k, f)
  }
  const digits = (() => {
    try { return new Intl.NumberFormat('en', { style: 'currency', currency }).resolvedOptions().maximumFractionDigits ?? 2 } catch { return 2 }
  })()
  return f.format(minor / 10 ** digits)
}

const dt = new Intl.DateTimeFormat(undefined, { year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false })
export const fmtTime = (t: string | number) => dt.format(new Date(t))
export const fmtInt = (n: number) => n.toLocaleString()

export function ago(t: string | number): string {
  const s = Math.max(0, (Date.now() - new Date(t).getTime()) / 1000)
  if (s < 60) return `${Math.floor(s)}s ago`
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`
  return `${Math.floor(s / 86400)}d ago`
}

export const errMsg = (e: unknown) => (e instanceof Error ? e.message : String(e))

/** Minimal async loader: refetches when deps change or `reload()` is called. Keeps stale data while refetching. */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T>()
  const [error, setError] = useState<string>()
  const [loading, setLoading] = useState(true)
  const [tick, setTick] = useState(0)
  useEffect(() => {
    let live = true
    setLoading(true)
    fn()
      .then((d) => { if (live) { setData(d); setError(undefined) } })
      .catch((e) => { if (live) setError(errMsg(e)) })
      .finally(() => { if (live) setLoading(false) })
    return () => { live = false }
  }, [...deps, tick]) // eslint-disable-line react-hooks/exhaustive-deps
  const reload = useCallback(() => setTick((t) => t + 1), [])
  return { data, error, loading, reload, setData }
}
