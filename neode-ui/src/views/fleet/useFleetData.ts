/** Composable encapsulating fleet telemetry data fetching, types, and utilities */

import { ref, computed, onMounted, onUnmounted, onActivated, onDeactivated, watch } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import type { ChartDataset } from '@/components/LineChart.vue'

// --- Types ---

export interface FleetNode {
  node_id: string
  node_name?: string | null
  hostname?: string | null
  server_url?: string | null
  version: string
  uptime_secs: number
  cpu_cores: number
  cpu_pct: number
  mem_pct: number
  disk_pct: number
  container_count: number
  running_count: number
  federation_peers: number
  recent_alerts: Array<{ rule: string; message: string; timestamp: string }>
  containers: Array<{ id: string; state: string; version: string }>
  reported_at: string
}

export interface FleetAlert {
  node_id: string
  rule: string
  message: string
  timestamp: string
}

export interface NodeHistoryEntry {
  timestamp: string
  cpu_pct: number
  mem_pct: number
  disk_pct: number
}

export type SortOption = 'status' | 'last-seen' | 'name'

// --- Utility Functions ---

export function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`
  const days = Math.floor(secs / 86400)
  const hours = Math.floor((secs % 86400) / 3600)
  const mins = Math.floor((secs % 3600) / 60)
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

export function timeAgo(dateStr: string): string {
  const now = Date.now()
  const then = new Date(dateStr).getTime()
  const diffMs = now - then
  if (diffMs < 0) return 'just now'
  const diffSecs = Math.floor(diffMs / 1000)
  if (diffSecs < 60) return `${diffSecs}s ago`
  const diffMins = Math.floor(diffSecs / 60)
  if (diffMins < 60) return `${diffMins}m ago`
  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return `${diffHours}h ago`
  const diffDays = Math.floor(diffHours / 24)
  return `${diffDays}d ago`
}

export function isOnline(reportedAt: string): boolean {
  const thirtyMinMs = 30 * 60 * 1000
  return Date.now() - new Date(reportedAt).getTime() < thirtyMinMs
}

export function healthBarClass(pct: number): string {
  if (pct >= 85) return 'monitoring-bar-danger'
  if (pct >= 60) return 'monitoring-bar-warn'
  return 'monitoring-bar-ok'
}

export function healthTextClass(pct: number): string {
  if (pct >= 85) return 'fleet-text-danger'
  if (pct >= 60) return 'fleet-text-warn'
  return ''
}

export function alertSeverityDot(rule: string): string {
  const critical = ['container_crash', 'disk_critical', 'node_offline']
  if (critical.includes(rule)) return 'bg-red-400'
  return 'bg-orange-400'
}

export function alertTypeLabel(rule: string): string {
  const labels: Record<string, string> = {
    container_crash: 'Container Crash',
    disk_critical: 'Disk Critical',
    disk_warning: 'Disk Warning',
    ram_high: 'High RAM',
    cpu_high: 'High CPU',
    node_offline: 'Node Offline',
    version_mismatch: 'Version Mismatch',
  }
  return labels[rule] ?? rule
}

export function formatTimestamp(ts: string): string {
  const d = new Date(ts)
  return d.toLocaleString()
}

export function getContainerState(node: FleetNode, appId: string): string | null {
  const container = (node.containers || []).find(c => c.id === appId)
  if (!container) return null
  return container.state
}

export function fleetNodeDisplayName(node: FleetNode): string {
  const name = node.node_name?.trim() || node.hostname?.trim()
  return name || node.node_id.slice(0, 8)
}

export function fleetNodeSubtitle(node: FleetNode): string {
  const host = node.hostname?.trim()
  if (host && host !== fleetNodeDisplayName(node)) return host
  return node.server_url?.trim() || node.node_id
}

export const SORT_OPTIONS: Array<{ label: string; value: SortOption }> = [
  { label: 'Status', value: 'status' },
  { label: 'Last Seen', value: 'last-seen' },
  { label: 'Name', value: 'name' },
]

export function sortFleetNodes(nodes: FleetNode[], sortBy: SortOption): FleetNode[] {
  const sorted = [...nodes]
  switch (sortBy) {
    case 'status':
      sorted.sort((a, b) => {
        const aOnline = isOnline(a.reported_at)
        const bOnline = isOnline(b.reported_at)
        if (aOnline !== bOnline) return aOnline ? -1 : 1
        return new Date(b.reported_at).getTime() - new Date(a.reported_at).getTime()
      })
      break
    case 'last-seen':
      sorted.sort((a, b) => new Date(b.reported_at).getTime() - new Date(a.reported_at).getTime())
      break
    case 'name':
      sorted.sort((a, b) => fleetNodeDisplayName(a).localeCompare(fleetNodeDisplayName(b)))
      break
  }
  return sorted
}

function numberOrZero(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

export function normalizeFleetNode(node: Partial<FleetNode>): FleetNode {
  return {
    node_id: typeof node.node_id === 'string' ? node.node_id : 'unknown',
    node_name: typeof node.node_name === 'string' ? node.node_name : null,
    hostname: typeof node.hostname === 'string' ? node.hostname : null,
    server_url: typeof node.server_url === 'string' ? node.server_url : null,
    version: typeof node.version === 'string' ? node.version : 'unknown',
    uptime_secs: numberOrZero(node.uptime_secs),
    cpu_cores: numberOrZero(node.cpu_cores),
    cpu_pct: numberOrZero(node.cpu_pct),
    mem_pct: numberOrZero(node.mem_pct),
    disk_pct: numberOrZero(node.disk_pct),
    container_count: numberOrZero(node.container_count),
    running_count: numberOrZero(node.running_count),
    federation_peers: numberOrZero(node.federation_peers),
    recent_alerts: Array.isArray(node.recent_alerts) ? node.recent_alerts : [],
    containers: Array.isArray(node.containers) ? node.containers : [],
    reported_at: typeof node.reported_at === 'string' ? node.reported_at : new Date(0).toISOString(),
  }
}

export function normalizeNodeHistoryResponse(data: {
  history?: NodeHistoryEntry[]
  entries?: NodeHistoryEntry[]
} | null | undefined): NodeHistoryEntry[] {
  if (Array.isArray(data?.history)) return data.history
  if (Array.isArray(data?.entries)) return data.entries
  return []
}

type FleetCache = {
  nodes: FleetNode[]
  fleetAlerts: FleetAlert[]
  lastRefreshed: string
  selectedNodeId: string | null
  sortBy: SortOption
}

const FLEET_CACHE_KEY = 'archipelago.fleet.cache.v1'

function readFleetCache(): Partial<FleetCache> {
  if (typeof window === 'undefined') return {}
  try {
    const raw = window.sessionStorage.getItem(FLEET_CACHE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as Partial<FleetCache>
    return {
      nodes: Array.isArray(parsed.nodes) ? parsed.nodes.map(normalizeFleetNode) : [],
      fleetAlerts: Array.isArray(parsed.fleetAlerts) ? parsed.fleetAlerts : [],
      lastRefreshed: typeof parsed.lastRefreshed === 'string' ? parsed.lastRefreshed : '',
      selectedNodeId: typeof parsed.selectedNodeId === 'string' ? parsed.selectedNodeId : null,
      sortBy: parsed.sortBy === 'last-seen' || parsed.sortBy === 'name' ? parsed.sortBy : 'status',
    }
  } catch {
    return {}
  }
}

function writeFleetCache(state: FleetCache) {
  if (typeof window === 'undefined') return
  try {
    window.sessionStorage.setItem(FLEET_CACHE_KEY, JSON.stringify(state))
  } catch {
    // Cache is opportunistic only.
  }
}

// --- Composable ---

export function useFleetData() {
  const cached = readFleetCache()
  const loading = ref(!(cached.nodes?.length ?? 0))
  const errorMessage = ref('')
  const nodes = ref<FleetNode[]>(cached.nodes ?? [])
  const fleetAlerts = ref<FleetAlert[]>(cached.fleetAlerts ?? [])
  const refreshing = ref(false)
  const alertsLoading = ref(false)
  const selectedNodeId = ref<string | null>(cached.selectedNodeId ?? null)
  const nodeHistory = ref<NodeHistoryEntry[]>([])
  const nodeHistoryLoading = ref(false)
  const autoRefresh = ref(true)
  const lastRefreshed = ref(cached.lastRefreshed ?? '')
  const sortBy = ref<SortOption>(cached.sortBy ?? 'status')
  const chartWidth = ref(300)
  let pollTimer: ReturnType<typeof setInterval> | null = null

  // --- Computed ---

  const onlineCount = computed(() => nodes.value.filter(n => isOnline(n.reported_at)).length)
  const offlineCount = computed(() => nodes.value.length - onlineCount.value)
  const healthyCount = computed(() => nodes.value.filter(n => n.recent_alerts.length === 0).length)

  const fleetHealthPct = computed(() => {
    if (!nodes.value.length) return 0
    return Math.round((healthyCount.value / nodes.value.length) * 100)
  })

  const avgCpu = computed(() => {
    if (!nodes.value.length) return 0
    return nodes.value.reduce((sum, n) => sum + n.cpu_pct, 0) / nodes.value.length
  })

  const avgMem = computed(() => {
    if (!nodes.value.length) return 0
    return nodes.value.reduce((sum, n) => sum + n.mem_pct, 0) / nodes.value.length
  })

  const avgDisk = computed(() => {
    if (!nodes.value.length) return 0
    return nodes.value.reduce((sum, n) => sum + n.disk_pct, 0) / nodes.value.length
  })

  const selectedNode = computed(() => {
    if (!selectedNodeId.value) return null
    return nodes.value.find(n => n.node_id === selectedNodeId.value) ?? null
  })

  const sortedNodes = computed(() => sortFleetNodes(nodes.value, sortBy.value))

  const allAppIds = computed(() => {
    const appSet = new Set<string>()
    for (const node of nodes.value) {
      for (const c of (node.containers || [])) {
        appSet.add(c.id)
      }
    }
    return Array.from(appSet).sort()
  })

  // Node history chart datasets
  const nodeHistoryLabels = computed(() => {
    return nodeHistory.value.map(h => {
      const d = new Date(h.timestamp)
      return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`
    })
  })

  const nodeHistoryCpuDatasets = computed<ChartDataset[]>(() => [{
    label: 'CPU',
    data: nodeHistory.value.map(h => h.cpu_pct),
    color: '#fb923c',
  }])

  const nodeHistoryMemDatasets = computed<ChartDataset[]>(() => [{
    label: 'RAM',
    data: nodeHistory.value.map(h => h.mem_pct),
    color: '#3b82f6',
  }])

  const nodeHistoryDiskDatasets = computed<ChartDataset[]>(() => [{
    label: 'Disk',
    data: nodeHistory.value.map(h => h.disk_pct),
    color: '#a78bfa',
  }])

  // --- Data Fetching ---

  async function fetchFleetStatus() {
    try {
      const data = await rpcClient.call<{ nodes: Partial<FleetNode>[] }>({
        method: 'telemetry.fleet-status',
      })
      if (data?.nodes) {
        nodes.value = data.nodes.map(normalizeFleetNode)
        lastRefreshed.value = new Date().toISOString()
        writeFleetCache({
          nodes: nodes.value,
          fleetAlerts: fleetAlerts.value,
          lastRefreshed: lastRefreshed.value,
          selectedNodeId: selectedNodeId.value,
          sortBy: sortBy.value,
        })
      }
    } catch (err) {
      if (loading.value) {
        errorMessage.value = err instanceof Error ? err.message : 'Failed to load fleet data'
      }
    }
  }

  async function fetchFleetAlerts() {
    alertsLoading.value = true
    try {
      const data = await rpcClient.call<{ alerts: FleetAlert[] }>({
        method: 'telemetry.fleet-alerts',
      })
      if (data?.alerts) {
        fleetAlerts.value = data.alerts
        writeFleetCache({
          nodes: nodes.value,
          fleetAlerts: fleetAlerts.value,
          lastRefreshed: lastRefreshed.value,
          selectedNodeId: selectedNodeId.value,
          sortBy: sortBy.value,
        })
      }
    } catch {
      // Non-critical, retry on next poll
    } finally {
      alertsLoading.value = false
    }
  }

  async function fetchNodeHistory(nodeId: string) {
    nodeHistoryLoading.value = true
    try {
      const data = await rpcClient.call<{ history?: NodeHistoryEntry[]; entries?: NodeHistoryEntry[] }>({
        method: 'telemetry.fleet-node-history',
        params: { node_id: nodeId },
      })
      nodeHistory.value = normalizeNodeHistoryResponse(data)
    } catch {
      // Non-critical
    } finally {
      nodeHistoryLoading.value = false
    }
  }

  async function refreshAll() {
    loading.value = !nodes.value.length
    refreshing.value = true
    errorMessage.value = ''
    try {
      await Promise.all([fetchFleetStatus(), fetchFleetAlerts()])
    } finally {
      loading.value = false
      refreshing.value = false
    }
  }

  function selectNode(nodeId: string) {
    if (selectedNodeId.value === nodeId) {
      selectedNodeId.value = null
      nodeHistory.value = []
    } else {
      selectedNodeId.value = nodeId
    }
    writeFleetCache({
      nodes: nodes.value,
      fleetAlerts: fleetAlerts.value,
      lastRefreshed: lastRefreshed.value,
      selectedNodeId: selectedNodeId.value,
      sortBy: sortBy.value,
    })
  }

  function toggleAutoRefresh() {
    autoRefresh.value = !autoRefresh.value
    if (autoRefresh.value) {
      startAutoRefresh()
    } else {
      stopAutoRefresh()
    }
  }

  function startAutoRefresh() {
    stopAutoRefresh()
    pollTimer = setInterval(async () => {
      await Promise.all([fetchFleetStatus(), fetchFleetAlerts()])
      if (selectedNodeId.value) {
        await fetchNodeHistory(selectedNodeId.value)
      }
    }, 60000)
  }

  function stopAutoRefresh() {
    if (pollTimer) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  }

  function exportFleetData() {
    const exportData = {
      exported_at: new Date().toISOString(),
      nodes: nodes.value,
      alerts: fleetAlerts.value,
    }
    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `fleet-telemetry-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  }

  function updateChartWidth() {
    const container = document.querySelector('.glass-card')
    if (container) {
      const cardWidth = container.clientWidth
      chartWidth.value = Math.max(Math.floor((cardWidth - 80) / 3), 200)
    }
  }

  // Fetch node history when selection changes
  watch(selectedNodeId, (newId) => {
    if (newId) {
      fetchNodeHistory(newId)
    } else {
      nodeHistory.value = []
    }
    writeFleetCache({
      nodes: nodes.value,
      fleetAlerts: fleetAlerts.value,
      lastRefreshed: lastRefreshed.value,
      selectedNodeId: selectedNodeId.value,
      sortBy: sortBy.value,
    })
  })

  watch(sortBy, () => {
    writeFleetCache({
      nodes: nodes.value,
      fleetAlerts: fleetAlerts.value,
      lastRefreshed: lastRefreshed.value,
      selectedNodeId: selectedNodeId.value,
      sortBy: sortBy.value,
    })
  })

  // --- Lifecycle ---

  // 02-11 gap closure: `startAutoRefresh()`'s 60s poll used to be armed only
  // in onMounted and disarmed only in onUnmounted — harmless before Fleet
  // joined KEEP_ALIVE_PATHS (02-04), since leaving the tab fully unmounted
  // this composable's owning component and onUnmounted fired every time.
  // Once Fleet is KeepAlive'd, onUnmounted never fires again after the first
  // visit, so the poll ran forever in the background regardless of tab
  // visibility — the exact off-screen-drain class 02-04's own audit
  // convention exists to prevent, missed here because that audit grepped
  // Fleet.vue itself (which has no lifecycle side effects of its own) and
  // never extended to this composable it delegates to. Arm/disarm now
  // follows activate/deactivate, mirroring Server.vue's vpnPollInterval
  // fix from 02-04; `startAutoRefresh()` already clears any existing timer
  // first, so calling it from both onMounted and onActivated on a fresh
  // KeepAlive-wrapped mount is safe and idempotent.
  function armFleetPoll() {
    if (autoRefresh.value) startAutoRefresh()
  }
  function disarmFleetPoll() {
    stopAutoRefresh()
  }

  onMounted(async () => {
    updateChartWidth()
    window.addEventListener('resize', updateChartWidth)
    await refreshAll()
    armFleetPoll()
  })

  onActivated(() => {
    armFleetPoll()
  })

  onDeactivated(() => {
    disarmFleetPoll()
  })

  onUnmounted(() => {
    disarmFleetPoll()
    window.removeEventListener('resize', updateChartWidth)
  })

  return {
    loading, refreshing, errorMessage, nodes, fleetAlerts, alertsLoading,
    selectedNodeId, selectedNode, nodeHistory, nodeHistoryLoading,
    autoRefresh, lastRefreshed, sortBy, chartWidth,
    onlineCount, offlineCount, healthyCount, fleetHealthPct,
    avgCpu, avgMem, avgDisk, sortedNodes, allAppIds,
    nodeHistoryLabels, nodeHistoryCpuDatasets, nodeHistoryMemDatasets, nodeHistoryDiskDatasets,
    refreshAll, selectNode, toggleAutoRefresh, exportFleetData,
  }
}
