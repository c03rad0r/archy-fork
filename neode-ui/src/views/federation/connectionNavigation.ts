import type { LocationQuery, RouteLocationRaw } from 'vue-router'

export function cloudSelection(query: LocationQuery) {
  const tab = typeof query.tab === 'string' && ['folders', 'mine', 'peers', 'paid'].includes(query.tab) ? query.tab : 'folders'
  const category = typeof query.category === 'string' && ['all', 'photos', 'music', 'documents'].includes(query.category) ? query.category : 'all'
  return { tab, category }
}

// Fixed route names only: never follow an arbitrary return URL from a link.
export function connectionReturn(query: LocationQuery): { label: string; to: RouteLocationRaw } {
  if (query.from === 'fleet') return { label: 'Fleet', to: { name: 'fleet' } }
  if (query.from === 'cloud') return { label: 'Cloud', to: { name: 'cloud', query: cloudSelection(query) } }
  if (query.from === 'peer-files' && typeof query.peer === 'string' && query.peer.length > 0 && query.peer.length <= 256 && !/[\u0000-\u0020/\\?#]/.test(query.peer)) {
    return { label: 'Peer files', to: { name: 'peer-files', params: { peerId: query.peer } } }
  }
  return { label: 'Web5', to: '/dashboard/web5' }
}
