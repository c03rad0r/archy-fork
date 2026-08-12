<template>
  <div class="pb-16 md:pb-6">
    <BackButton :label="backLabel" desktop-margin="mb-6" @click="router.push(backTarget)" />

    <div class="hidden md:block mb-8">
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-3xl font-bold text-white mb-2">{{ t('monitoring.title') }}</h1>
          <p class="text-white/70">{{ t('monitoring.subtitle') }}</p>
        </div>
        <div class="flex gap-2">
          <button class="glass-button text-sm px-4 py-2" @click="exportMetrics('csv')">
            {{ t('monitoring.exportCsv') }}
          </button>
          <button class="glass-button text-sm px-4 py-2" @click="exportMetrics('json')">
            {{ t('monitoring.exportJson') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Summary Cards -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">{{ t('monitoring.cpu') }}</p>
        <p class="text-2xl font-bold text-white">{{ liveSystem.cpu_percent.toFixed(1) }}%</p>
        <p class="text-xs text-white/40">{{ t('monitoring.load') }} {{ liveSystem.load_avg_1.toFixed(2) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">{{ t('monitoring.memory') }}</p>
        <p class="text-2xl font-bold text-white">{{ memPercent }}%</p>
        <p class="text-xs text-white/40">{{ formatBytes(liveSystem.mem_used_bytes) }} / {{ formatBytes(liveSystem.mem_total_bytes) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">{{ t('monitoring.diskUsage') }}</p>
        <p class="text-2xl font-bold text-white">{{ diskPercent }}%</p>
        <p class="text-xs text-white/40">{{ formatBytes(liveSystem.disk_used_bytes) }} / {{ formatBytes(liveSystem.disk_total_bytes) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">{{ t('monitoring.network') }}</p>
        <p class="text-2xl font-bold text-white">{{ formatBytes(liveSystem.net_rx_bytes) }}</p>
        <p class="text-xs text-white/40">TX: {{ formatBytes(liveSystem.net_tx_bytes) }}</p>
      </div>
    </div>

    <!-- Charts -->
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-6">
      <div class="glass-card p-5">
        <h3 class="text-sm font-medium text-white/80 mb-3">{{ t('monitoring.cpuUsage') }}</h3>
        <LineChart
          :datasets="cpuDatasets"
          :labels="timeLabels"
          :width="chartWidth"
          :height="180"
          :y-max="100"
        />
      </div>
      <div class="glass-card p-5">
        <h3 class="text-sm font-medium text-white/80 mb-3">{{ t('monitoring.memoryUsage') }}</h3>
        <LineChart
          :datasets="memDatasets"
          :labels="timeLabels"
          :width="chartWidth"
          :height="180"
          :y-max="100"
        />
      </div>
      <div class="glass-card p-5">
        <h3 class="text-sm font-medium text-white/80 mb-3">{{ t('monitoring.networkIo') }}</h3>
        <LineChart
          :datasets="netDatasets"
          :labels="timeLabels"
          :width="chartWidth"
          :height="180"
        />
      </div>
      <div class="glass-card p-5">
        <h3 class="text-sm font-medium text-white/80 mb-3">{{ t('monitoring.rpcLatency') }}</h3>
        <LineChart
          :datasets="latencyDatasets"
          :labels="timeLabels"
          :width="chartWidth"
          :height="180"
        />
      </div>
    </div>

    <!-- Alert History -->
    <div class="glass-card p-5 mb-6">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-sm font-medium text-white/80">{{ t('monitoring.alertHistory') }}</h3>
        <button
          class="glass-button text-xs px-3 py-1"
          @click="showAlertConfig = !showAlertConfig"
        >
          {{ showAlertConfig ? t('monitoring.hideConfig') : t('common.configure') }}
        </button>
      </div>

      <!-- Alert Rule Configuration -->
      <div v-if="showAlertConfig" class="mb-4 space-y-2">
        <div
          v-for="rule in alertRules"
          :key="rule.kind"
          class="flex items-center gap-3 p-3 bg-white/5 rounded-lg"
        >
          <ToggleSwitch :model-value="rule.enabled" @update:model-value="toggleAlertRule(rule.kind, $event)" />
          <div class="flex-1 min-w-0">
            <p class="text-sm text-white">{{ ruleLabel(rule.kind) }}</p>
            <p class="text-xs text-white/40">{{ rule.description }}</p>
          </div>
          <div class="flex items-center gap-2">
            <input
              type="number"
              :value="rule.threshold"
              class="monitoring-threshold-input"
              @change="updateThreshold(rule.kind, ($event.target as HTMLInputElement).value)"
            />
            <span class="text-xs text-white/40">{{ ruleUnit(rule.kind) }}</span>
          </div>
        </div>
      </div>

      <!-- Fired Alerts List -->
      <div v-if="!alerts.length" class="text-white/40 text-sm py-4 text-center">
        {{ t('monitoring.noAlerts') }}
      </div>
      <div v-else class="space-y-2 max-h-64 overflow-y-auto">
        <div
          v-for="alert in alerts"
          :key="alert.id"
          class="flex items-start gap-3 p-3 bg-white/5 rounded-lg"
          :class="{ 'opacity-50': alert.acknowledged }"
        >
          <span
            class="inline-block w-2 h-2 rounded-full mt-1.5 flex-shrink-0"
            :class="alertDotColor(alert.kind)"
          ></span>
          <div class="flex-1 min-w-0">
            <p class="text-sm text-white">{{ alert.message }}</p>
            <p class="text-xs text-white/40">{{ formatAlertTime(alert.timestamp) }}</p>
          </div>
          <button
            v-if="!alert.acknowledged"
            class="text-xs text-white/40 hover:text-white/70 flex-shrink-0"
            @click="acknowledgeAlert(alert.id)"
          >
            {{ t('common.dismiss') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Container Resource Breakdown -->
    <div class="glass-card p-5 mb-6">
      <h3 class="text-sm font-medium text-white/80 mb-4">{{ t('monitoring.containerResources') }}</h3>
      <div v-if="!containers.length" class="text-white/40 text-sm py-4 text-center">
        {{ t('monitoring.noContainerMetrics') }}
      </div>
      <div v-else class="space-y-3">
        <div
          v-for="c in containers"
          :key="c.name"
          class="flex items-center gap-4 p-3 bg-white/5 rounded-lg"
        >
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white truncate">{{ c.name }}</p>
            <div class="flex gap-4 mt-1 text-xs text-white/50">
              <span>CPU: {{ c.cpu_percent.toFixed(1) }}%</span>
              <span>Mem: {{ formatBytes(c.mem_used_bytes) }}</span>
              <span>Net: {{ formatBytes(c.net_rx_bytes) }} / {{ formatBytes(c.net_tx_bytes) }}</span>
            </div>
          </div>
          <div class="monitoring-bar-container">
            <div
              class="monitoring-bar-fill"
              :style="{ width: Math.min(c.cpu_percent, 100) + '%' }"
              :class="c.cpu_percent > 80 ? 'monitoring-bar-danger' : c.cpu_percent > 50 ? 'monitoring-bar-warn' : 'monitoring-bar-ok'"
            ></div>
          </div>
        </div>
      </div>
    </div>

    <!-- System Health Timeline -->
    <div class="glass-card p-5">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-sm font-medium text-white/80">{{ t('monitoring.systemHealth') }}</h3>
        <div class="flex items-center gap-2 text-xs text-white/40">
          <span class="inline-block w-2 h-2 rounded-full bg-green-400"></span> {{ t('common.healthy') }}
          <span class="inline-block w-2 h-2 rounded-full bg-orange-400 ml-2"></span> {{ t('common.elevated') }}
          <span class="inline-block w-2 h-2 rounded-full bg-red-400 ml-2"></span> {{ t('common.critical') }}
        </div>
      </div>
      <div class="flex gap-0.5 h-6">
        <div
          v-for="(entry, idx) in healthTimeline"
          :key="idx"
          class="flex-1 rounded-sm transition-colors"
          :class="healthColor(entry)"
          :title="entry.label"
        ></div>
      </div>
      <div class="flex justify-between mt-1 text-xs text-white/30">
        <span>{{ historyMinutesAgo }}m ago</span>
        <span>Now</span>
      </div>
    </div>

    <p class="text-xs text-white/30 mt-4 text-center">
      {{ t('monitoring.refreshFooter') }} &middot; {{ t('monitoring.wsConnections', { count: current?.ws_connections ?? 0 }) }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import { useHomeStatusStore } from '@/stores/homeStatus'
import BackButton from '@/components/BackButton.vue'
import LineChart from '@/components/LineChart.vue'
import ToggleSwitch from '@/components/ToggleSwitch.vue'
import type { ChartDataset } from '@/components/LineChart.vue'

interface SystemMetrics {
  cpu_percent: number
  mem_used_bytes: number
  mem_total_bytes: number
  disk_used_bytes: number
  disk_total_bytes: number
  net_rx_bytes: number
  net_tx_bytes: number
  load_avg_1: number
  load_avg_5: number
  load_avg_15: number
}

interface ContainerMetrics {
  name: string
  cpu_percent: number
  mem_used_bytes: number
  mem_limit_bytes: number
  net_rx_bytes: number
  net_tx_bytes: number
  block_read_bytes: number
  block_write_bytes: number
}

interface MetricSnapshot {
  timestamp: number
  system: SystemMetrics
  containers: ContainerMetrics[]
  rpc_latency_ms: number
  ws_connections: number
}

interface HistoryResponse {
  resolution: string
  count: number
  data: MetricSnapshot[]
}

interface AlertRule {
  kind: string
  threshold: number
  enabled: boolean
  description: string
}

interface FiredAlert {
  id: string
  kind: string
  message: string
  value: number
  threshold: number
  timestamp: number
  acknowledged: boolean
}

const router = useRouter()
const route = useRoute()
const { t } = useI18n()

// Monitoring is reachable from Web5 (the monitoring link) AND from the Home
// "System" card (which passes ?from=home). The back button follows the entry
// point so it returns where the user came from instead of always Web5.
const cameFromHome = computed(() => route.query.from === 'home')
const backTarget = computed(() => (cameFromHome.value ? '/dashboard' : '/dashboard/web5'))
const backLabel = computed(() => (cameFromHome.value ? t('common.back') : 'Web5'))
const homeStatus = useHomeStatusStore()

// Cached: revisits paint the last snapshot/chart/alerts instantly and the 5s
// poll revalidates behind them; errors keep the last-known values.
const currentRes = useCachedResource<MetricSnapshot>({
  key: 'monitoring.current',
  fetcher: async (signal) => {
    const data = await rpcClient.call<MetricSnapshot | { status: string }>({
      method: 'monitoring.current', signal, dedup: true, maxRetries: 1,
    })
    if (!data || !('system' in data)) throw new Error('metrics not ready')
    return data
  },
  persist: true, // system/container/rpc metrics — no identity/money payload
})
const historyRes = useCachedResource<MetricSnapshot[]>({
  key: 'monitoring.history.minute60',
  fetcher: async (signal) => {
    const data = await rpcClient.call<HistoryResponse>({
      method: 'monitoring.history', params: { resolution: 'minute', count: 60 },
      signal, dedup: true, maxRetries: 1,
    })
    return data?.data ?? []
  },
  persist: true, // historical metric snapshots — no identity/money payload
})
const alertsRes = useCachedResource<FiredAlert[]>({
  key: 'monitoring.alerts',
  fetcher: async (signal) => {
    const data = await rpcClient.call<{ alerts: FiredAlert[] }>({
      method: 'monitoring.alerts', params: { count: 50 }, signal, dedup: true, maxRetries: 1,
    })
    return (data?.alerts ?? []).reverse()
  },
  persist: true, // alert metadata (kind/message/threshold) — no identity/money payload
})
const alertRulesRes = useCachedResource<AlertRule[]>({
  key: 'monitoring.alert-rules',
  fetcher: async (signal) => {
    const data = await rpcClient.call<{ rules: AlertRule[] }>({
      method: 'monitoring.alert-rules', signal, dedup: true, maxRetries: 1,
    })
    return data?.rules ?? []
  },
  persist: true, // static alert-rule config — no identity/money payload
})
const current = computed(() => currentRes.data.value)
const history = computed(() => historyRes.data.value ?? [])
const containers = computed<ContainerMetrics[]>(() => currentRes.data.value?.containers ?? [])
const alerts = computed(() => alertsRes.data.value ?? [])
const alertRules = computed(() => alertRulesRes.data.value ?? [])
const showAlertConfig = ref(false)
const chartWidth = ref(380)
let pollTimer: ReturnType<typeof setInterval> | null = null

const liveSystem = computed<SystemMetrics>(() => ({
  cpu_percent: homeStatus.stats.cpuPercent,
  mem_used_bytes: homeStatus.stats.memUsed,
  mem_total_bytes: homeStatus.stats.memTotal,
  disk_used_bytes: homeStatus.stats.diskUsed,
  disk_total_bytes: homeStatus.stats.diskTotal,
  net_rx_bytes: current.value?.system.net_rx_bytes ?? 0,
  net_tx_bytes: current.value?.system.net_tx_bytes ?? 0,
  load_avg_1: homeStatus.stats.loadAvg1,
  load_avg_5: homeStatus.stats.loadAvg5,
  load_avg_15: homeStatus.stats.loadAvg15,
}))

const memPercent = computed(() => {
  if (!liveSystem.value.mem_total_bytes) return '--'
  return ((liveSystem.value.mem_used_bytes / liveSystem.value.mem_total_bytes) * 100).toFixed(1)
})

const diskPercent = computed(() => {
  if (!liveSystem.value.disk_total_bytes) return '--'
  return ((liveSystem.value.disk_used_bytes / liveSystem.value.disk_total_bytes) * 100).toFixed(1)
})

const historyMinutesAgo = computed(() => history.value.length || 60)

const timeLabels = computed(() => {
  if (!history.value.length) return []
  return history.value.map((s) => {
    const d = new Date(s.timestamp * 1000)
    return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`
  })
})

const cpuDatasets = computed<ChartDataset[]>(() => [{
  label: 'CPU',
  data: history.value.map((s) => s.system.cpu_percent),
  color: '#fb923c',
}])

const memDatasets = computed<ChartDataset[]>(() => [{
  label: 'Memory',
  data: history.value.map((s) =>
    s.system.mem_total_bytes > 0
      ? (s.system.mem_used_bytes / s.system.mem_total_bytes) * 100
      : 0,
  ),
  color: '#3b82f6',
}])

const netDatasets = computed<ChartDataset[]>(() => [
  {
    label: 'RX',
    data: history.value.map((s) => s.system.net_rx_bytes),
    color: '#4ade80',
  },
  {
    label: 'TX',
    data: history.value.map((s) => s.system.net_tx_bytes),
    color: '#f59e0b',
  },
])

const latencyDatasets = computed<ChartDataset[]>(() => [{
  label: 'Latency',
  data: history.value.map((s) => s.rpc_latency_ms),
  color: '#a78bfa',
}])

interface HealthEntry {
  cpu: number
  mem: number
  label: string
}

const healthTimeline = computed<HealthEntry[]>(() => {
  if (!history.value.length) {
    return Array.from({ length: 60 }, () => ({ cpu: 0, mem: 0, label: 'No data' }))
  }
  return history.value.map((s) => {
    const memPct = s.system.mem_total_bytes > 0
      ? (s.system.mem_used_bytes / s.system.mem_total_bytes) * 100
      : 0
    const d = new Date(s.timestamp * 1000)
    const time = `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}`
    return {
      cpu: s.system.cpu_percent,
      mem: memPct,
      label: `${time} — CPU: ${s.system.cpu_percent.toFixed(1)}%, Mem: ${memPct.toFixed(1)}%`,
    }
  })
})

function healthColor(entry: HealthEntry): string {
  if (entry.cpu > 90 || entry.mem > 90) return 'bg-red-400/60'
  if (entry.cpu > 70 || entry.mem > 70) return 'bg-orange-400/40'
  return 'bg-green-400/30'
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${bytes} B`
}

function ruleLabel(kind: string): string {
  const labels: Record<string, string> = {
    disk_usage: t('monitoring.diskUsage'),
    ram_usage: t('monitoring.ramUsage'),
    container_crash: t('monitoring.containerCrash'),
    backend_error_spike: t('monitoring.rpcLatencySpike'),
    ssl_cert_expiry: t('monitoring.sslCertExpiry'),
  }
  return labels[kind] ?? kind
}

function ruleUnit(kind: string): string {
  const units: Record<string, string> = {
    disk_usage: '%',
    ram_usage: '%',
    container_crash: '',
    backend_error_spike: 'ms',
    ssl_cert_expiry: 'days',
  }
  return units[kind] ?? ''
}

function alertDotColor(kind: string): string {
  if (kind === 'container_crash' || kind === 'ssl_cert_expiry') return 'bg-red-400'
  if (kind === 'disk_usage' || kind === 'ram_usage') return 'bg-orange-400'
  return 'bg-yellow-400'
}

function formatAlertTime(timestamp: number): string {
  const d = new Date(timestamp * 1000)
  return d.toLocaleString()
}

async function exportMetrics(format: 'csv' | 'json') {
  try {
    const data = await rpcClient.call<{ csv?: string; data?: unknown[]; count: number }>({
      method: 'monitoring.export',
      params: { format, resolution: 'minute', count: 1440 },
    })
    let blob: Blob
    let filename: string
    if (format === 'csv' && data?.csv) {
      blob = new Blob([data.csv], { type: 'text/csv' })
      filename = `archipelago-metrics-${new Date().toISOString().slice(0, 10)}.csv`
    } else {
      blob = new Blob([JSON.stringify(data?.data ?? [], null, 2)], { type: 'application/json' })
      filename = `archipelago-metrics-${new Date().toISOString().slice(0, 10)}.json`
    }
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = filename
    a.click()
    URL.revokeObjectURL(url)
  } catch {
    if (import.meta.env.DEV) console.warn('Failed to export metrics')
  }
}

async function toggleAlertRule(kind: string, enabled: boolean) {
  try {
    await rpcClient.call({ method: 'monitoring.configure-alert', params: { kind, enabled } })
    await alertRulesRes.refresh()
  } catch {
    // Non-critical
  }
}

async function updateThreshold(kind: string, value: string) {
  const threshold = parseFloat(value)
  if (isNaN(threshold) || threshold <= 0) return
  try {
    await rpcClient.call({ method: 'monitoring.configure-alert', params: { kind, threshold } })
    await alertRulesRes.refresh()
  } catch {
    // Non-critical
  }
}

async function acknowledgeAlert(id: string) {
  try {
    await rpcClient.call({ method: 'monitoring.acknowledge-alert', params: { id } })
    await alertsRes.refresh()
  } catch {
    // Non-critical
  }
}

function updateChartWidth() {
  const container = document.querySelector('.glass-card')
  if (container) {
    chartWidth.value = Math.max(container.clientWidth - 40, 200)
  }
}

onMounted(() => {
  updateChartWidth()
  window.addEventListener('resize', updateChartWidth)

  // The cached resources fetch themselves on first use; the poll keeps the
  // live view fresh (refreshes dedup in the store).
  void homeStatus.refreshSystemStats()
  pollTimer = setInterval(() => {
    void homeStatus.refreshSystemStats()
    void currentRes.refresh()
    void historyRes.refresh()
    void alertsRes.refresh()
  }, 5000)
})

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer)
  window.removeEventListener('resize', updateChartWidth)
})
</script>
