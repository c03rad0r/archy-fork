/** User-friendly node display name. Prefers name, falls back to "Node-XXXX" from DID hash. */
export function nodeName(node: { name?: string | null; did: string }): string {
  if (node.name) return node.name
  const suffix = node.did.replace(/^did:key:z6Mk/, '').slice(-6).toUpperCase()
  return `Node-${suffix}`
}

/** Look up display name from DID using a node list. */
export function nodeNameFromDid(did: string, nodes: { name?: string; did: string }[]): string {
  const node = nodes.find(n => n.did === did)
  if (node) return nodeName(node)
  const suffix = did.replace(/^did:key:z6Mk/, '').slice(-6).toUpperCase()
  return `Node-${suffix}`
}

export function shortDid(did: string): string {
  if (did.length <= 24) return did
  return did.slice(0, 16) + '...' + did.slice(-8)
}

export function timeAgo(iso: string): string {
  const seconds = Math.floor((Date.now() - new Date(iso).getTime()) / 1000)
  if (seconds < 60) return 'just now'
  if (seconds < 3600) return Math.floor(seconds / 60) + 'm ago'
  if (seconds < 86400) return Math.floor(seconds / 3600) + 'h ago'
  return Math.floor(seconds / 86400) + 'd ago'
}

export function formatTimeAgo(iso: string): string {
  if (!iso || iso === 'never') return 'never'
  const ms = Date.now() - new Date(iso).getTime()
  if (ms < 60000) return 'just now'
  if (ms < 3600000) return `${Math.floor(ms / 60000)}m ago`
  if (ms < 86400000) return `${Math.floor(ms / 3600000)}h ago`
  return `${Math.floor(ms / 86400000)}d ago`
}

export function formatBytes(bytes?: number): string {
  if (bytes == null || bytes === 0) return '--'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let i = 0
  let val = bytes
  while (val >= 1024 && i < units.length - 1) {
    val /= 1024
    i++
  }
  return val.toFixed(1) + ' ' + units[i]
}

export function formatUptime(secs: number): string {
  const days = Math.floor(secs / 86400)
  const hours = Math.floor((secs % 86400) / 3600)
  if (days > 0) return `${days}d ${hours}h`
  const mins = Math.floor((secs % 3600) / 60)
  return `${hours}h ${mins}m`
}

export function trustBadgeClass(level: string): string {
  switch (level) {
    case 'trusted': return 'bg-green-400/20 text-green-400'
    case 'observer': return 'bg-blue-400/20 text-blue-400'
    case 'untrusted': return 'bg-white/10 text-white/50'
    default: return 'bg-white/10 text-white/50'
  }
}

export function isOnline(node: { last_seen?: string }): boolean {
  if (!node.last_seen) return false
  const lastSeen = new Date(node.last_seen).getTime()
  const tenMinutesAgo = Date.now() - 10 * 60 * 1000
  return lastSeen > tenMinutesAgo
}
