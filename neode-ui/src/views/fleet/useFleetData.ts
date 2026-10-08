/** Composable encapsulating fleet telemetry data fetching, types, and utilities */

import { ref, computed, onMounted, onUnmounted, onActivated, onDeactivated, watch } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import type { ChartDataset } from '@/components/LineChart.vue'

// --- Types ---

export interface FleetNode {
  node_id: string
  identity_authenticated?: boolean
  source?: string
  trust_level?: string
  node_name?: string | null
  hostname?: string | null
  server_url?: string | null
  version: string
  uptime_secs: number | null
  cpu_cores: number
  cpu_pct: number | null
  mem_pct: number | null
  disk_pct: number | null
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

export function formatUptime(secs: number | null): string {
  if (secs === null) return 'Unavailable'
  if (secs < 60) return `${secs}s`
  const days = Math.floor(secs / 86400)
  const hours = Math.floor((secs % 86400) / 3600)
  const mins = Math.floor((secs % 3600) / 60)
  if (days > 0) return `${days}d ${hours}h`
  if (hours > 0) return `${hours}h ${mins}m`
  return `${mins}m`
}

export function timeAgo(dateStr: string, now = Date.now()): string {
  const then = new Date(dateStr).getTime()
  if (!Number.isFinite(then) || then > now + 60_000) return 'Unknown'
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

export function fleetStatus(reportedAt: string, now = Date.now()): 'online' | 'stale' | 'unknown' {
  const age = now - new Date(reportedAt).getTime()
  if (!Number.isFinite(age) || age < -60_000) return 'unknown'
  return age < 30 * 60 * 1000 ? 'online' : 'stale'
}

export function isOnline(reportedAt: string, now = Date.now()): boolean {
  return fleetStatus(reportedAt, now) === 'online'
}

export function formatMetric(value: number | null, digits = 0): string {
  return value === null ? '—' : `${value.toFixed(digits)}%`
}

export function averageMetric(nodes: FleetNode[], field: 'cpu_pct' | 'mem_pct' | 'disk_pct', now = Date.now()): number | null {
  const values = nodes.filter(n => isOnline(n.reported_at, now)).map(n => n[field]).filter((v): v is number => v !== null && Number.isFinite(v))
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : null
}

export function healthBarClass(pct: number | null): string {
  if (pct === null) return ''
  if (pct >= 85) return 'monitoring-bar-danger'
  if (pct >= 60) return 'monitoring-bar-warn'
  return 'monitoring-bar-ok'
}

export function healthTextClass(pct: number | null): string {
  if (pct === null) return ''
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

export function fleetSourceLabel(node: FleetNode): string {
  if (node.source === 'collector') return 'Collector · identity unverified'
  if (node.source === 'federation' && node.trust_level === 'trusted' && node.identity_authenticated === true) return 'Trusted federation'
  return 'Source unverified'
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

function reportTime(node: FleetNode, now: number): number {
  return fleetStatus(node.reported_at, now) === 'unknown' ? 0 : new Date(node.reported_at).getTime()
}

export function sortFleetNodes(nodes: FleetNode[], sortBy: SortOption, now = Date.now()): FleetNode[] {
  const sorted = [...nodes]
  switch (sortBy) {
    case 'status':
      sorted.sort((a, b) => {
        const rank = { online: 0, unknown: 1, stale: 2 }
        return rank[fleetStatus(a.reported_at, now)] - rank[fleetStatus(b.reported_at, now)]
      })
      break
    case 'last-seen':
      sorted.sort((a, b) => reportTime(b, now) - reportTime(a, now))
      break
    case 'name':
      sorted.sort((a, b) => fleetNodeDisplayName(a).localeCompare(fleetNodeDisplayName(b)))
      break
  }
  return sorted
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null
}

function percentageOrNull(value: unknown): number | null {
  const number = numberOrNull(value)
  return number !== null && number <= 100 ? number : null
}

function numberOrZero(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

export function normalizeFleetNode(node: Partial<FleetNode>): FleetNode {
  return {
    node_id: typeof node.node_id === 'string' ? node.node_id : 'unknown',
    identity_authenticated: node.identity_authenticated === true,
    source: typeof node.source === 'string' ? node.source : 'unknown',
    trust_level: typeof node.trust_level === 'string' ? node.trust_level : 'unverified',
    node_name: typeof node.node_name === 'string' ? node.node_name : null,
    hostname: typeof node.hostname === 'string' ? node.hostname : null,
    server_url: typeof node.server_url === 'string' ? node.server_url : null,
    version: typeof node.version === 'string' ? node.version : 'unknown',
    uptime_secs: numberOrNull(node.uptime_secs),
    cpu_cores: numberOrZero(node.cpu_cores),
    cpu_pct: percentageOrNull(node.cpu_pct),
    mem_pct: percentageOrNull(node.mem_pct),
    disk_pct: percentageOrNull(node.disk_pct),
    container_count: numberOrZero(node.container_count),
    running_count: numberOrZero(node.running_count),
    federation_peers: numberOrZero(node.federation_peers),
    recent_alerts: Array.isArray(node.recent_alerts) ? node.recent_alerts : [],
    containers: Array.isArray(node.containers) ? node.containers : [],
    reported_at: typeof node.reported_at === 'string' ? node.reported_at : '',
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

const FLEET_CACHE_KEY = 'archipelago.fleet.cache.v2'

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
  const now = ref(Date.now())
  let clockTimer: ReturnType<typeof setInterval> | null = null
  let active = true
  const cached = readFleetCache()
  const loading = ref(!(cached.nodes?.length ?? 0))
  const errorMessage = ref('')
  const nodes = ref<FleetNode[]>(cached.nodes ?? [])
  const fleetAlerts = ref<FleetAlert[]>(cached.fleetAlerts ?? [])
  const refreshing = ref(false)
  const alertsLoading = ref(false)
  const alertsErrorMessage = ref('')
  const selectedNodeId = ref<string | null>(cached.selectedNodeId ?? null)
  const nodeHistory = ref<NodeHistoryEntry[]>([])
  const nodeHistoryLoading = ref(false)
  const nodeHistoryMessage = ref('')
  const autoRefresh = ref(true)
  const lastRefreshed = ref(cached.lastRefreshed ?? '')
  const sortBy = ref<SortOption>(cached.sortBy ?? 'status')
  const chartWidth = ref(300)
  let pollTimer: ReturnType<typeof setInterval> | null = null
  let historyRequest = 0
  let statusRequest = 0
  let alertsRequest = 0
  let refreshRequest = 0
  let disposed = false

  // --- Computed ---

  const onlineCount = computed(() => nodes.value.filter(n => isOnline(n.reported_at, now.value)).length)
  const offlineCount = computed(() => nodes.value.filter(n => fleetStatus(n.reported_at, now.value) === 'stale').length)
  const unknownCount = computed(() => nodes.value.filter(n => fleetStatus(n.reported_at, now.value) === 'unknown').length)
  const healthyCount = computed(() => nodes.value.filter(n => isOnline(n.reported_at, now.value) && n.cpu_pct !== null && n.mem_pct !== null && n.disk_pct !== null && n.recent_alerts.length === 0).length)

  const fleetHealthPct = computed(() => {
    if (!nodes.value.length) return 0
    return Math.round((healthyCount.value / nodes.value.length) * 100)
  })

  const avgCpu = computed(() => averageMetric(nodes.value, 'cpu_pct', now.value))

  const avgMem = computed(() => averageMetric(nodes.value, 'mem_pct', now.value))

  const avgDisk = computed(() => averageMetric(nodes.value, 'disk_pct', now.value))

  const selectedNode = computed(() => {
    if (!selectedNodeId.value) return null
    return nodes.value.find(n => n.node_id === selectedNodeId.value) ?? null
  })

  const sortedNodes = computed(() => sortFleetNodes(nodes.value, sortBy.value, now.value))

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
    const request = ++statusRequest
    try {
      const data = await rpcClient.call<{ nodes: Partial<FleetNode>[] }>({
        method: 'telemetry.fleet-status',
      })
      if (disposed || request !== statusRequest) return
      if (!Array.isArray(data?.nodes) || data.nodes.some(node => !node || typeof node !== 'object')) {
        throw new Error('Invalid fleet response')
      }
      nodes.value = data.nodes.map(normalizeFleetNode)
      lastRefreshed.value = new Date().toISOString()
      errorMessage.value = ''
      writeFleetCache({
        nodes: nodes.value,
        fleetAlerts: fleetAlerts.value,
        lastRefreshed: lastRefreshed.value,
        selectedNodeId: selectedNodeId.value,
        sortBy: sortBy.value,
      })
    } catch {
      if (!disposed && request === statusRequest) {
        errorMessage.value = nodes.value.length
          ? 'Fleet refresh failed. Showing the last received reports; current node status could not be checked.'
          : 'Fleet data is unavailable. Check your connection and retry.'
      }
    }
  }

  async function fetchFleetAlerts() {
    const request = ++alertsRequest
    alertsLoading.value = true
    try {
      const data = await rpcClient.call<{ alerts: FleetAlert[] }>({
        method: 'telemetry.fleet-alerts',
      })
      if (disposed || request !== alertsRequest) return
      if (!Array.isArray(data?.alerts) || data.alerts.some(alert => !alert ||
        typeof alert.node_id !== 'string' || typeof alert.rule !== 'string' ||
        typeof alert.message !== 'string' || typeof alert.timestamp !== 'string')) {
        throw new Error('Invalid fleet alerts response')
      }
      {
        fleetAlerts.value = data.alerts
        alertsErrorMessage.value = ''
        writeFleetCache({
          nodes: nodes.value,
          fleetAlerts: fleetAlerts.value,
          lastRefreshed: lastRefreshed.value,
          selectedNodeId: selectedNodeId.value,
          sortBy: sortBy.value,
        })
      }
    } catch {
      if (!disposed && request === alertsRequest) {
        alertsErrorMessage.value = fleetAlerts.value.length
          ? 'Alert refresh failed. Showing the last received alerts.'
          : 'Fleet alerts are unavailable. Retry refresh to check for alerts.'
      }
    } finally {
      if (!disposed && request === alertsRequest) alertsLoading.value = false
    }
  }

  async function fetchNodeHistory(nodeId: string) {
    const request = ++historyRequest
    nodeHistoryLoading.value = true
    try {
      const data = await rpcClient.call<{ history?: NodeHistoryEntry[]; entries?: NodeHistoryEntry[]; history_available?: boolean }>({
        method: 'telemetry.fleet-node-history',
        params: { node_id: nodeId },
      })
      if (request === historyRequest && selectedNodeId.value === nodeId) {
        if (data.history_available === false) {
          nodeHistory.value = []
          nodeHistoryMessage.value = 'History is unavailable for this node.'
        } else {
          nodeHistory.value = normalizeNodeHistoryResponse(data)
          nodeHistoryMessage.value = ''
        }
      }
    } catch {
      if (request === historyRequest && selectedNodeId.value === nodeId) {
        nodeHistoryMessage.value = nodeHistory.value.length
          ? 'History refresh failed. Showing the last received history.'
          : 'History is unavailable for this node.'
      }
    } finally {
      if (request === historyRequest) nodeHistoryLoading.value = false
    }
  }

  async function refreshAll() {
    const request = ++refreshRequest
    loading.value = !nodes.value.length
    refreshing.value = true
    try {
      await Promise.all([fetchFleetStatus(), fetchFleetAlerts()])
    } finally {
      if (!disposed && request === refreshRequest) {
        loading.value = false
        refreshing.value = false
      }
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
    if (autoRefresh.value && active) {
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
    historyRequest++
    nodeHistory.value = []
    nodeHistoryLoading.value = false
    nodeHistoryMessage.value = ''
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
    active = true
    now.value = Date.now()
    if (clockTimer) clearInterval(clockTimer)
    clockTimer = setInterval(() => { now.value = Date.now() }, 15_000)
    if (autoRefresh.value) startAutoRefresh()
  }
  function disarmFleetPoll() {
    active = false
    if (clockTimer) clearInterval(clockTimer)
    clockTimer = null
    stopAutoRefresh()
  }

  onMounted(async () => {
    updateChartWidth()
    window.addEventListener('resize', updateChartWidth)
    armFleetPoll()
    await refreshAll()
  })

  onActivated(() => {
    armFleetPoll()
  })

  onDeactivated(() => {
    disarmFleetPoll()
  })

  onUnmounted(() => {
    disposed = true
    ++historyRequest
    disarmFleetPoll()
    window.removeEventListener('resize', updateChartWidth)
  })

  return {
    loading, refreshing, errorMessage, nodes, fleetAlerts, alertsLoading, alertsErrorMessage,
    selectedNodeId, selectedNode, nodeHistory, nodeHistoryLoading, nodeHistoryMessage,
    autoRefresh, lastRefreshed, sortBy, chartWidth, now,
    onlineCount, offlineCount, unknownCount, healthyCount, fleetHealthPct,
    avgCpu, avgMem, avgDisk, sortedNodes, allAppIds,
    nodeHistoryLabels, nodeHistoryCpuDatasets, nodeHistoryMemDatasets, nodeHistoryDiskDatasets,
    refreshAll, selectNode, toggleAutoRefresh, exportFleetData,
  }
}
