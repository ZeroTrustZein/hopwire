export type Scheme = 'fps' | 'chaps' | 'bacs' | 'card' | 'internal' | 'other'

export interface Txn {
  tx_id: string
  ts_ms: number
  amount_minor: number
  currency: string
  debtor: string
  creditor: string
  scheme?: Scheme
  device_id?: string | null
  ip?: string | null
  debtor_customer_hash?: string | null
  creditor_customer_hash?: string | null
  creditor_account_opened_at_ms?: number | null
}

export type NodeKind = 'account' | 'device' | 'ip' | 'customer'
export type EdgeKind = 'payment' | 'uses'
export type Rule =
  | 'cycle'
  | 'mule_fan_in'
  | 'pass_through'
  | 'fan_out'
  | 'velocity'
  | 'shared_identifier'
  | 'ml'

export interface PathEdge {
  kind: EdgeKind
  from: string
  to: string
  tx_id: string
  ts_ms: number
  amount_minor: number
}

export interface Finding {
  rule: Rule
  score: number
  reason: string
  path: PathEdge[]
}

export type Decision = 'allow' | 'review' | 'block'
export type AlertStatus = 'open' | 'escalated' | 'closed'
export type DispositionLabel = 'true_positive' | 'false_positive'

export interface Disposition {
  alert_id: string
  label: DispositionLabel
  analyst: string
  note: string | null
  created_at: string
}

export interface AlertSummary {
  id: string
  tx_id: string
  created_at: string
  ts_ms: number
  score: number
  decision: Decision
  rules: Rule[]
  status: AlertStatus
  debtor: string
  creditor: string
  amount_minor: number
  currency: string
  case_id: string | null
}

export interface AlertDetail extends AlertSummary {
  findings: Finding[]
  txn: Txn
  disposition: Disposition | null
}

export interface SubNode {
  id: string
  kind: NodeKind
  hub: boolean
}

export interface Subgraph {
  nodes: SubNode[]
  edges: PathEdge[]
}

export type CaseStatus = 'open' | 'in_review' | 'closed'

export interface Case {
  id: string
  title: string
  status: CaseStatus
  assignee: string | null
  created_at: string
  updated_at: string
  alert_ids: string[]
}

export interface CaseDetail extends Case {
  alerts: AlertSummary[]
  notes: Note[]
}

export interface Note {
  id: string
  case_id: string
  author: string
  body: string
  created_at: string
}

export interface EngineStats {
  nodes: number
  edges: number
  hubs: number
  processed: number
  watermark_ms: number
}

export interface AuditVerify {
  ok: boolean
  entries: number
  broken_at: number | null
}

export interface Paginated<T> {
  items: T[]
  total: number
}

export interface AlertFilters {
  status?: AlertStatus | ''
  rule?: Rule | ''
  min_score?: number | ''
  limit?: number
  offset?: number
}

export interface CaseFilters {
  status?: CaseStatus | ''
  limit?: number
  offset?: number
}
