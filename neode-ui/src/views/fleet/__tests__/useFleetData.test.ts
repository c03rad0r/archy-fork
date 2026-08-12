import { describe, expect, it, vi } from 'vitest'
import {
  fleetNodeDisplayName,
  fleetNodeSubtitle,
  isOnline,
  normalizeFleetNode,
  normalizeNodeHistoryResponse,
  sortFleetNodes,
  type FleetNode,
} from '../useFleetData'

function node(id: string, reportedAt: string): FleetNode {
  return {
    node_id: id,
    node_name: null,
    hostname: null,
    server_url: null,
    version: '1.8-alpha',
    uptime_secs: 60,
    cpu_cores: 4,
    cpu_pct: 10,
    mem_pct: 20,
    disk_pct: 30,
    container_count: 2,
    running_count: 2,
    federation_peers: 1,
    recent_alerts: [],
    containers: [],
    reported_at: reportedAt,
  }
}

describe('fleet data helpers', () => {
  it('treats nodes reported within 30 minutes as online', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-10T12:00:00Z'))

    expect(isOnline('2026-06-10T11:45:00Z')).toBe(true)
    expect(isOnline('2026-06-10T11:20:00Z')).toBe(false)

    vi.useRealTimers()
  })

  it('sorts status with online nodes first, then latest report', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-10T12:00:00Z'))
    const nodes = [
      node('offline', '2026-06-10T10:00:00Z'),
      node('online-old', '2026-06-10T11:45:00Z'),
      node('online-new', '2026-06-10T11:59:00Z'),
    ]

    expect(sortFleetNodes(nodes, 'status').map(n => n.node_id)).toEqual([
      'online-new',
      'online-old',
      'offline',
    ])

    vi.useRealTimers()
  })

  it('sorts by name alphabetically', () => {
    const zulu = node('zulu', '2026-06-10T11:59:00Z')
    zulu.node_name = 'Workshop'
    const alpha = node('alpha', '2026-06-10T11:59:00Z')
    alpha.node_name = 'Kitchen'

    expect(sortFleetNodes([zulu, alpha], 'name').map(n => n.node_id)).toEqual(['alpha', 'zulu'])
  })

  it('normalizes older telemetry reports with missing metric and container fields', () => {
    const normalized = normalizeFleetNode({
      node_id: 'legacy-node',
      version: '1.8-alpha',
      reported_at: '2026-06-10T11:59:00Z',
    })

    expect(normalized.node_id).toBe('legacy-node')
    expect(normalized.node_name).toBeNull()
    expect(normalized.hostname).toBeNull()
    expect(normalized.server_url).toBeNull()
    expect(normalized.cpu_pct).toBe(0)
    expect(normalized.mem_pct).toBe(0)
    expect(normalized.disk_pct).toBe(0)
    expect(normalized.containers).toEqual([])
    expect(normalized.recent_alerts).toEqual([])
  })

  it('uses node name, hostname, then node id for fleet display labels', () => {
    const named = normalizeFleetNode({
      node_id: 'abcdef123456',
      node_name: 'Kitchen Node',
      hostname: 'kitchen-node',
      server_url: 'https://192.0.2.20',
    })
    const hostOnly = normalizeFleetNode({
      node_id: '123456abcdef',
      hostname: 'workshop-node',
      server_url: 'https://192.0.2.21',
    })
    const idOnly = normalizeFleetNode({ node_id: 'feedfacecafebeef' })

    expect(fleetNodeDisplayName(named)).toBe('Kitchen Node')
    expect(fleetNodeSubtitle(named)).toBe('kitchen-node')
    expect(fleetNodeDisplayName(hostOnly)).toBe('workshop-node')
    expect(fleetNodeSubtitle(hostOnly)).toBe('https://192.0.2.21')
    expect(fleetNodeDisplayName(idOnly)).toBe('feedface')
    expect(fleetNodeSubtitle(idOnly)).toBe('feedfacecafebeef')
  })

  it('normalizes node history responses from backend entries or legacy history fields', () => {
    const entry = { timestamp: '2026-06-10T11:59:00Z', cpu_pct: 1, mem_pct: 2, disk_pct: 3 }

    expect(normalizeNodeHistoryResponse({ entries: [entry] })).toEqual([entry])
    expect(normalizeNodeHistoryResponse({ history: [entry] })).toEqual([entry])
    expect(normalizeNodeHistoryResponse({})).toEqual([])
  })
})
