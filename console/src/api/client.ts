import type {
  AlertDetail, AlertFilters, AlertSummary, AuditVerify, Case, CaseDetail, CaseFilters,
  CaseStatus, Disposition, DispositionLabel, EngineStats, Note, Paginated, PsiReport,
  SarExportResponse, Subgraph,
} from './types'

const BASE = (import.meta.env.VITE_API_URL ?? '/').replace(/\/$/, '')
const MOCK = import.meta.env.VITE_MOCK === '1'
const KEY = 'aegis.apiKey'

export class ApiError extends Error {
  status: number
  constructor(status: number, message: string) {
    super(message)
    this.status = status
  }
}

export const auth = {
  get: () => sessionStorage.getItem(KEY),
  set: (k: string) => sessionStorage.setItem(KEY, k),
  clear: () => sessionStorage.removeItem(KEY),
}

/** Called on 401 so the app can bounce to /login. */
let onUnauthorized = () => {}
export const setUnauthorizedHandler = (fn: () => void) => { onUnauthorized = fn }

export async function request<T>(method: string, path: string, body?: unknown, key = auth.get()): Promise<T> {
  if (MOCK) {
    const { mockRequest } = await import('./mock')
    return mockRequest(method, path, body) as Promise<T>
  }
  const res = await fetch(BASE + path, {
    method,
    headers: {
      ...(key ? { Authorization: `Bearer ${key}` } : {}),
      ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`
    try { msg = ((await res.json()) as { error?: string }).error ?? msg } catch { /* non-JSON body */ }
    if (res.status === 401) { auth.clear(); onUnauthorized() }
    throw new ApiError(res.status, msg)
  }
  return res.json() as Promise<T>
}

const qs = (p: object) => {
  const s = new URLSearchParams()
  for (const [k, v] of Object.entries(p)) if (v !== undefined && v !== '' && v !== null) s.set(k, String(v))
  const str = s.toString()
  return str ? `?${str}` : ''
}

export const api = {
  stats: (key?: string) => request<EngineStats>('GET', '/v1/stats', undefined, key),
  auditVerify: () => request<AuditVerify>('GET', '/v1/audit/verify'),
  alerts: (f: AlertFilters) => request<Paginated<AlertSummary>>('GET', `/v1/alerts${qs(f)}`),
  alert: (id: string) => request<AlertDetail>('GET', `/v1/alerts/${id}`),
  subgraph: (id: string, hops = 2) => request<Subgraph>('GET', `/v1/alerts/${id}/subgraph?hops=${hops}`),
  dispose: (id: string, b: { label: DispositionLabel; analyst: string; note?: string }) =>
    request<Disposition>('POST', `/v1/alerts/${id}/disposition`, b),
  cases: (f: CaseFilters) => request<Paginated<Case>>('GET', `/v1/cases${qs(f)}`),
  case: (id: string) => request<CaseDetail>('GET', `/v1/cases/${id}`),
  createCase: (b: { title: string; alert_ids: string[]; assignee?: string }) => request<Case>('POST', '/v1/cases', b),
  patchCase: (id: string, b: { status?: CaseStatus; assignee?: string }) => request<Case>('PATCH', `/v1/cases/${id}`, b),
  addNote: (id: string, b: { author: string; body: string }) => request<Note>('POST', `/v1/cases/${id}/notes`, b),
  generateSar: (id: string) => request<SarExportResponse>('POST', `/v1/alerts/${id}/sar`, {}),
  drift: () => request<PsiReport>('GET', '/v1/model/drift'),
}
