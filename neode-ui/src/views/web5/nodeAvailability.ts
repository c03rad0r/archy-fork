/** Reachability is an observation, not proof of a continuous outage. */
export interface ReachabilitySample { reachable: boolean | null; checkedAt: number; lastSeen?: number }
export type NodeAvailability = 'online' | 'unreachable' | 'unknown'
export const REACHABILITY_MAX_AGE = 90_000

export function validContact(value: unknown, now: number): number | undefined {
  const timestamp = typeof value === 'number' ? value : typeof value === 'string' ? Date.parse(value) : NaN
  return Number.isFinite(timestamp) && timestamp > 0 && timestamp <= now + 60_000 ? timestamp : undefined
}
export function nodeAvailability(sample: ReachabilitySample | undefined, now: number): NodeAvailability {
  if (!sample || !Number.isFinite(sample.checkedAt) || now - sample.checkedAt > REACHABILITY_MAX_AGE || sample.checkedAt > now + 60_000) return 'unknown'
  return sample.reachable === true ? 'online' : sample.reachable === false ? 'unreachable' : 'unknown'
}
export function lastContactLabel(value: unknown, now: number): string {
  const timestamp = validContact(value, now)
  if (!timestamp) return 'No confirmed contact'
  const seconds = Math.max(0, Math.floor((now - timestamp) / 1000))
  if (seconds < 60) return 'Last seen just now'
  if (seconds < 3600) return `Last seen ${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `Last seen ${Math.floor(seconds / 3600)}h ago`
  return `Last seen ${Math.floor(seconds / 86400)}d ago`
}
export function availabilityRank(status: NodeAvailability): number {
  return status === 'online' ? 0 : status === 'unknown' ? 1 : 2
}
