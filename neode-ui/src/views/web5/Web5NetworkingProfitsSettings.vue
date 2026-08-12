<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import ToggleSwitch from '@/components/ToggleSwitch.vue'
import BackButton from '@/components/BackButton.vue'
import LineChart from '@/components/LineChart.vue'
import type { ChartDataset } from '@/components/LineChart.vue'

const router = useRouter()

// Mirrors `crate::streaming::pricing::ServicePricing` (Metric is rename_all =
// "lowercase", so it round-trips verbatim back to streaming.configure-service).
type Metric = 'bytes' | 'milliseconds' | 'requests'

interface ServicePricing {
  service_id: string
  name: string
  metric: Metric
  step_size: number
  price_per_step: number
  min_steps: number
  enabled: boolean
  description: string
  accepted_mints: string[]
}

// ── Tabs ──
const tab = ref<'dashboard' | 'configure'>('dashboard')

// ── Configure state ──
const services = ref<ServicePricing[]>([])
const loading = ref(true)
const loadError = ref('')
const savingId = ref<string | null>(null)
const statusMsg = ref('')
const statusIsError = ref(false)

// "Free everything" is the default — every service ships disabled. The banner
// reassures the user nothing is being charged for until they opt in.
const allFree = computed(() => services.value.every((s) => !s.enabled) && !tollgate.value?.enabled)

function showStatus(msg: string, isError: boolean) {
  statusMsg.value = msg
  statusIsError.value = isError
  setTimeout(() => { statusMsg.value = '' }, 5000)
}

/** Human label for one priced step, e.g. "MB", "minute", "request". */
function unitLabel(metric: Metric, stepSize: number): string {
  if (metric === 'bytes') {
    if (stepSize === 1_073_741_824) return 'GB'
    if (stepSize === 1_048_576) return 'MB'
    if (stepSize === 1024) return 'KB'
    return `${stepSize.toLocaleString()} bytes`
  }
  if (metric === 'milliseconds') {
    if (stepSize === 3_600_000) return 'hour'
    if (stepSize === 60_000) return 'minute'
    if (stepSize === 1000) return 'second'
    return `${stepSize.toLocaleString()} ms`
  }
  // requests
  return stepSize === 1 ? 'request' : `${stepSize.toLocaleString()} requests`
}

function minimumNote(svc: ServicePricing): string {
  if (svc.min_steps <= 0) return ''
  return `Minimum purchase: ${svc.min_steps.toLocaleString()} ${unitLabel(svc.metric, svc.step_size)}${svc.min_steps > 1 ? 's' : ''}`
}

async function load() {
  loading.value = true
  loadError.value = ''
  try {
    const res = await rpcClient.call<{ services: ServicePricing[] }>({ method: 'streaming.list-services' })
    services.value = (res.services || []).map((s) => ({
      ...s,
      // Price must stay >= 1 sat: the backend rejects price_per_step == 0.
      price_per_step: Math.max(1, s.price_per_step),
    }))
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : 'Failed to load services'
  } finally {
    loading.value = false
  }
}

async function saveService(svc: ServicePricing) {
  if (svc.price_per_step < 1) svc.price_per_step = 1
  savingId.value = svc.service_id
  try {
    await rpcClient.call({
      method: 'streaming.configure-service',
      params: {
        service_id: svc.service_id,
        name: svc.name,
        metric: svc.metric,
        step_size: svc.step_size,
        price_per_step: svc.price_per_step,
        min_steps: svc.min_steps,
        enabled: svc.enabled,
        description: svc.description,
        accepted_mints: svc.accepted_mints,
      },
    })
    showStatus(
      svc.enabled
        ? `Charging ${svc.price_per_step} sats per ${unitLabel(svc.metric, svc.step_size)} for ${svc.name}.`
        : `${svc.name} is now free.`,
      false,
    )
  } catch (e) {
    showStatus(e instanceof Error ? e.message : 'Failed to save', true)
    // Reload so the UI reflects the persisted truth after a failed write.
    void load()
  } finally {
    savingId.value = null
  }
}

// ── TollGate (paid WiFi on the OpenWrt gateway) ──
interface TollGateStatus {
  installed: boolean
  enabled?: boolean
  step_size_ms?: number
  price_per_step?: number
  min_steps?: number
  mint_url?: string
}

const tollgate = ref<TollGateStatus | null>(null)
const tollgateChecked = ref(false)
const tollgateSaving = ref(false)
const tollgatePrice = ref(10)
const tollgateEnabled = ref(false)

const tollgateUnit = computed(() => {
  const ms = tollgate.value?.step_size_ms || 60_000
  if (ms === 3_600_000) return 'hour'
  if (ms === 60_000) return 'minute'
  if (ms === 1000) return 'second'
  return `${ms.toLocaleString()} ms`
})

async function loadTollgate() {
  try {
    const res = await rpcClient.call<{ tollgate?: TollGateStatus }>({ method: 'openwrt.get-status' })
    tollgate.value = res.tollgate || null
    tollgateEnabled.value = !!res.tollgate?.enabled
    tollgatePrice.value = Math.max(1, res.tollgate?.price_per_step || 10)
  } catch {
    // No gateway configured (or unreachable) — the card shows the setup hint.
    tollgate.value = null
  } finally {
    tollgateChecked.value = true
  }
}

async function saveTollgate() {
  if (tollgatePrice.value < 1) tollgatePrice.value = 1
  tollgateSaving.value = true
  try {
    await rpcClient.call({
      method: 'openwrt.provision-tollgate',
      params: {
        enabled: tollgateEnabled.value,
        // Real backend reads price_sats; the mock reads price_per_step —
        // send both so the same call works against either.
        price_sats: tollgatePrice.value,
        price_per_step: tollgatePrice.value,
        step_size_ms: tollgate.value?.step_size_ms || 60_000,
        min_steps: tollgate.value?.min_steps || 1,
      },
    })
    showStatus(
      tollgateEnabled.value
        ? `TollGate WiFi is charging ${tollgatePrice.value} sats per ${tollgateUnit.value}.`
        : 'TollGate WiFi is now free.',
      false,
    )
    void loadTollgate()
  } catch (e) {
    showStatus(e instanceof Error ? e.message : 'Failed to save TollGate settings', true)
  } finally {
    tollgateSaving.value = false
  }
}

// ── Dashboard state ──
type ProfitSource = 'content_sale' | 'routing_fee' | 'streaming_revenue'

interface ProfitEntry {
  source: ProfitSource
  amount_sats: number
  timestamp: string
  description?: string
}

interface ProfitsSummary {
  total_sats: number
  content_sales_sats: number
  routing_fees_sats: number
  streaming_revenue_sats?: number
  recent?: ProfitEntry[]
}

interface SessionsSummary {
  total_active: number
  total_revenue_sats: number
  revenue_by_service: Record<string, number>
}

const profits = ref<ProfitsSummary | null>(null)
const sessionsSummary = ref<SessionsSummary | null>(null)
const dashboardLoading = ref(true)

async function loadDashboard() {
  dashboardLoading.value = true
  try {
    const [p, s] = await Promise.all([
      rpcClient.call<ProfitsSummary>({ method: 'wallet.networking-profits' }).catch(() => null),
      rpcClient.call<SessionsSummary>({ method: 'streaming.list-sessions' }).catch(() => null),
    ])
    profits.value = p
    sessionsSummary.value = s
  } finally {
    dashboardLoading.value = false
  }
}

function formatSats(n: number | undefined | null): string {
  return (n ?? 0).toLocaleString()
}

// 7 daily buckets, oldest → newest, one series per earning source. A week of
// daily totals is the sweet spot here: earnings are sparse events (unlike the
// second-by-second system metrics on Monitoring), so finer buckets would just
// draw noise, and a longer window would flatten a new node's first sats.
const DAY_MS = 86_400_000
const WINDOW_DAYS = 7

const dayLabels = computed(() => {
  const names = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']
  const labels: string[] = []
  for (let i = WINDOW_DAYS - 1; i >= 0; i--) {
    labels.push(names[new Date(Date.now() - i * DAY_MS).getDay()] as string)
  }
  return labels
})

function bucketize(source: ProfitSource): number[] {
  const buckets = new Array(WINDOW_DAYS).fill(0)
  const now = Date.now()
  for (const e of profits.value?.recent || []) {
    if (e.source !== source) continue
    const t = Date.parse(e.timestamp)
    if (Number.isNaN(t)) continue
    const age = Math.floor((now - t) / DAY_MS)
    if (age < 0 || age >= WINDOW_DAYS) continue
    buckets[WINDOW_DAYS - 1 - age] += e.amount_sats
  }
  return buckets
}

const earningsDatasets = computed<ChartDataset[]>(() => [
  { label: 'Streaming', data: bucketize('streaming_revenue'), color: '#f97316' },
  { label: 'Content sales', data: bucketize('content_sale'), color: '#3b82f6' },
  { label: 'Routing fees', data: bucketize('routing_fee'), color: '#a78bfa' },
])

const hasRecentEarnings = computed(() =>
  earningsDatasets.value.some((d) => d.data.some((v) => v > 0)),
)

// Revenue-by-service bars, largest first, scaled to the biggest earner.
const serviceBars = computed(() => {
  const by = sessionsSummary.value?.revenue_by_service || {}
  const entries = Object.entries(by).sort((a, b) => b[1] - a[1])
  const max = entries[0]?.[1] || 1
  return entries.map(([id, sats]) => ({
    id,
    name: services.value.find((s) => s.service_id === id)?.name || id,
    sats,
    pct: Math.max(4, Math.round((sats / max) * 100)),
  }))
})

// ── Chart sizing (same measure-the-card approach as Monitoring) ──
const chartCard = ref<HTMLElement | null>(null)
const chartWidth = ref(600)

function updateChartWidth() {
  chartWidth.value = Math.max(280, (chartCard.value?.clientWidth || 640) - 40)
}

onMounted(() => {
  void load()
  void loadTollgate()
  void loadDashboard()
  updateChartWidth()
  window.addEventListener('resize', updateChartWidth)
})

onUnmounted(() => {
  window.removeEventListener('resize', updateChartWidth)
})
</script>

<template>
  <div class="pb-6">
    <BackButton label="Back to Web5" @click="router.push('/dashboard/web5')" />

    <div class="mb-4">
      <h1 class="text-3xl font-bold text-white">Networking Profits</h1>
    </div>

    <!-- Tabs: Dashboard | Configure -->
    <div class="flex gap-1 mb-6 border-b border-white/10">
      <button
        @click="tab = 'dashboard'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="tab === 'dashboard' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        Dashboard
      </button>
      <button
        @click="tab = 'configure'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="tab === 'configure' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        Configure
      </button>
    </div>

    <!-- ============ DASHBOARD ============ -->
    <div v-show="tab === 'dashboard'">
      <div v-if="dashboardLoading" class="glass-card p-6 text-white/60 text-sm">Loading earnings…</div>
      <template v-else>
        <!-- Stat tiles -->
        <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
          <div class="glass-card p-5">
            <p class="text-xs text-white/50 uppercase tracking-wide">Total earned</p>
            <p class="text-2xl font-bold text-white">{{ formatSats(profits?.total_sats) }}</p>
            <p class="text-xs text-white/40">sats, all time</p>
          </div>
          <div class="glass-card p-5">
            <p class="text-xs text-white/50 uppercase tracking-wide">Streaming</p>
            <p class="text-2xl font-bold text-orange-400">{{ formatSats(profits?.streaming_revenue_sats) }}</p>
            <p class="text-xs text-white/40">sats from paid services</p>
          </div>
          <div class="glass-card p-5">
            <p class="text-xs text-white/50 uppercase tracking-wide">Content sales</p>
            <p class="text-2xl font-bold text-blue-400">{{ formatSats(profits?.content_sales_sats) }}</p>
            <p class="text-xs text-white/40">sats from ecash sales</p>
          </div>
          <div class="glass-card p-5">
            <p class="text-xs text-white/50 uppercase tracking-wide">Routing fees</p>
            <p class="text-2xl font-bold text-violet-400">{{ formatSats(profits?.routing_fees_sats) }}</p>
            <p class="text-xs text-white/40">sats from Lightning</p>
          </div>
        </div>

        <!-- Earnings chart -->
        <div ref="chartCard" class="glass-card p-5 mb-6">
          <div class="flex items-center justify-between mb-3 flex-wrap gap-2">
            <h3 class="text-sm font-medium text-white/80">Earnings — last 7 days</h3>
            <div class="flex items-center gap-4">
              <span v-for="d in earningsDatasets" :key="d.label" class="flex items-center gap-1.5 text-xs text-white/60">
                <span class="w-2.5 h-2.5 rounded-full" :style="{ backgroundColor: d.color }"></span>
                {{ d.label }}
              </span>
            </div>
          </div>
          <LineChart
            v-if="hasRecentEarnings"
            :datasets="earningsDatasets"
            :labels="dayLabels"
            :width="chartWidth"
            :height="200"
          />
          <div v-else class="py-10 text-center text-white/40 text-sm">
            No earnings in the last 7 days — enable a paid service under Configure to start earning.
          </div>
        </div>

        <!-- Sessions + revenue by service -->
        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div class="glass-card p-5">
            <p class="text-xs text-white/50 uppercase tracking-wide mb-1">Active sessions</p>
            <p class="text-4xl font-bold text-white mb-1">{{ sessionsSummary?.total_active ?? 0 }}</p>
            <p class="text-xs text-white/40">peers currently paying for services</p>
            <p class="text-sm text-white/70 mt-4">
              Session revenue:
              <span class="text-white font-medium">{{ formatSats(sessionsSummary?.total_revenue_sats) }} sats</span>
            </p>
          </div>
          <div class="glass-card p-5 lg:col-span-2">
            <h3 class="text-sm font-medium text-white/80 mb-4">Revenue by service</h3>
            <div v-if="serviceBars.length === 0" class="py-6 text-center text-white/40 text-sm">
              No service revenue yet.
            </div>
            <div v-else class="space-y-3">
              <div v-for="bar in serviceBars" :key="bar.id">
                <div class="flex items-center justify-between text-xs mb-1">
                  <span class="text-white/80">{{ bar.name }}</span>
                  <span class="text-white/50">{{ formatSats(bar.sats) }} sats</span>
                </div>
                <div class="h-2 rounded-full bg-white/5 overflow-hidden">
                  <div class="h-full rounded-full bg-orange-500/70" :style="{ width: bar.pct + '%' }"></div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </template>
    </div>

    <!-- ============ CONFIGURE ============ -->
    <div v-show="tab === 'configure'">
      <!-- Intro copy, boxed so it reads as its own thing rather than page dressing -->
      <div class="glass-card p-4 mb-6">
        <p class="text-sm text-white/70 leading-relaxed">
          Control what your node charges other peers for. By default everything is shared for
          free — turn a service on to start earning sats (ecash) for it. Payments are collected
          as Cashu tokens through your node's wallet.
        </p>
      </div>

      <!-- Status message -->
      <div
        v-if="statusMsg"
        role="status"
        aria-live="polite"
        class="mb-4 p-3 rounded-lg text-sm"
        :class="statusIsError ? 'bg-red-500/20 text-red-300' : 'bg-green-500/20 text-green-300'"
      >
        {{ statusMsg }}
      </div>

      <!-- Everything-free reassurance banner -->
      <div
        v-if="!loading && allFree"
        class="mb-6 p-4 rounded-lg bg-green-500/10 border border-green-500/20 flex items-center gap-3"
      >
        <span class="text-xl">✓</span>
        <p class="text-sm text-green-200">
          Everything is free. Your node isn't charging for anything — enable a service below to
          start earning.
        </p>
      </div>

      <div v-if="loading" class="glass-card p-6 text-white/60 text-sm">Loading services…</div>
      <div v-else-if="loadError" class="glass-card p-6 text-red-300 text-sm">{{ loadError }}</div>

      <div v-else class="space-y-4">
        <div v-for="svc in services" :key="svc.service_id" class="glass-card p-6">
          <div class="flex items-start justify-between gap-4 mb-3">
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-white">{{ svc.name }}</h2>
              <p class="text-sm text-white/60 mt-0.5">{{ svc.description }}</p>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <span class="text-xs" :class="svc.enabled ? 'text-orange-400' : 'text-white/40'">
                {{ svc.enabled ? 'Paid' : 'Free' }}
              </span>
              <ToggleSwitch :model-value="svc.enabled" @update:model-value="(v) => (svc.enabled = v)" />
            </div>
          </div>

          <div class="flex flex-col sm:flex-row sm:items-end gap-3">
            <div class="flex-1" :class="{ 'opacity-40 pointer-events-none': !svc.enabled }">
              <label class="text-xs text-white/50 block mb-1">Price</label>
              <div class="flex items-center gap-2">
                <input
                  v-model.number="svc.price_per_step"
                  type="number"
                  min="1"
                  step="1"
                  :disabled="!svc.enabled"
                  class="w-28 bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-orange-500/50"
                />
                <span class="text-sm text-white/70">sats per {{ unitLabel(svc.metric, svc.step_size) }}</span>
              </div>
              <p v-if="minimumNote(svc)" class="text-xs text-white/40 mt-1">{{ minimumNote(svc) }}</p>
            </div>
            <button
              @click="saveService(svc)"
              :disabled="savingId === svc.service_id"
              class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm disabled:opacity-50"
            >
              {{ savingId === svc.service_id ? 'Saving…' : 'Save' }}
            </button>
          </div>
        </div>

        <!-- TollGate: paid WiFi on the OpenWrt gateway -->
        <div v-if="tollgateChecked" class="glass-card p-6">
          <div class="flex items-start justify-between gap-4 mb-3">
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-white">TollGate WiFi</h2>
              <p class="text-sm text-white/60 mt-0.5">
                Sell WiFi access on your OpenWrt gateway — visitors pay per {{ tollgateUnit }} in ecash.
              </p>
            </div>
            <div v-if="tollgate?.installed" class="flex items-center gap-2 shrink-0">
              <span class="text-xs" :class="tollgateEnabled ? 'text-orange-400' : 'text-white/40'">
                {{ tollgateEnabled ? 'Paid' : 'Free' }}
              </span>
              <ToggleSwitch :model-value="tollgateEnabled" @update:model-value="(v) => (tollgateEnabled = v)" />
            </div>
          </div>

          <div v-if="tollgate?.installed" class="flex flex-col sm:flex-row sm:items-end gap-3">
            <div class="flex-1" :class="{ 'opacity-40 pointer-events-none': !tollgateEnabled }">
              <label class="text-xs text-white/50 block mb-1">Price</label>
              <div class="flex items-center gap-2">
                <input
                  v-model.number="tollgatePrice"
                  type="number"
                  min="1"
                  step="1"
                  :disabled="!tollgateEnabled"
                  class="w-28 bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white focus:outline-none focus:border-orange-500/50"
                />
                <span class="text-sm text-white/70">sats per {{ tollgateUnit }}</span>
              </div>
            </div>
            <button
              @click="saveTollgate"
              :disabled="tollgateSaving"
              class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm disabled:opacity-50"
            >
              {{ tollgateSaving ? 'Saving…' : 'Save' }}
            </button>
          </div>
          <div v-else class="flex flex-col sm:flex-row sm:items-center gap-3">
            <p class="text-sm text-white/50 flex-1">
              TollGate isn't set up yet — it needs an OpenWrt gateway paired with this node.
            </p>
            <button
              @click="router.push('/dashboard/server/openwrt')"
              class="glass-button px-4 py-2 rounded-lg text-sm shrink-0"
            >
              Set up gateway
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
