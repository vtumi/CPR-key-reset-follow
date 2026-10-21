import { request } from '../request'

export interface Budget { weekly_limit_usd: string, weekly_used_usd: string }
export interface Link {
  key_id: string
  account_id: string
  baseline: { reset: number }
  paused: boolean
  early_auto: boolean | null
  pending: { kind: 'confirm' | 'ready' | 'unknown' | 'changed' } | null
}
export interface Snapshot {
  state: {
    early_auto: boolean
    links: Record<string, Link>
    accounts: Record<string, { failure_since: number | null }>
  }
  keys: { id: string, name: string }[]
  accounts: { account_id: string, name: string, email: string | null, provider_id: string, authentication_kind: string, plan_type: string | null }[]
  budgets: Record<string, Budget>
}
export const load = () => request<Snapshot>('GET', 'state')
export const command = (body: object) => request<{ ok: boolean }>('POST', 'action', body)
