// Shared utility functions for Web5 subcomponents

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export function formatTxTime(timestamp: number): string {
  if (!timestamp) return ''
  const date = new Date(timestamp * 1000)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMin = Math.floor(diffMs / 60000)
  if (diffMin < 1) return 'Just now'
  if (diffMin < 60) return `${diffMin}m ago`
  const diffHours = Math.floor(diffMin / 60)
  if (diffHours < 24) return `${diffHours}h ago`
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return `${diffDays}d ago`
  return date.toLocaleDateString()
}

export function formatMessageTime(ts: string): string {
  try {
    const d = new Date(ts)
    const now = new Date()
    const diff = now.getTime() - d.getTime()
    if (diff < 60000) return 'Just now'
    if (diff < 3600000) return `${Math.floor(diff / 60000)}m ago`
    if (diff < 86400000) return `${Math.floor(diff / 3600000)}h ago`
    return d.toLocaleDateString()
  } catch {
    return ts
  }
}

export async function safeClipboardWrite(text: string): Promise<void> {
  // navigator.clipboard is unavailable on HTTP (non-secure contexts)
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return
    }
  } catch { /* fall through to textarea fallback */ }
  const ta = document.createElement('textarea')
  ta.value = text
  ta.style.position = 'fixed'
  ta.style.opacity = '0'
  document.body.appendChild(ta)
  ta.select()
  document.execCommand('copy')
  document.body.removeChild(ta)
}

export function isMediaType(mime: string): boolean {
  return mime.startsWith('audio/') || mime.startsWith('video/')
}

export function getAccessType(access: string | { paid: { price_sats: number } }): 'free' | 'peers_only' | 'paid' {
  if (typeof access === 'string') {
    if (access === 'peersonly' || access === 'peers_only') return 'peers_only'
    if (access === 'paid') return 'paid'
    return 'free'
  }
  if (access && typeof access === 'object' && 'paid' in access) return 'paid'
  return 'free'
}

export function getItemPrice(access: string | { paid: { price_sats: number } }): number {
  if (typeof access === 'object' && access && 'paid' in access) {
    return access.paid.price_sats
  }
  return 0
}
