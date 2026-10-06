<template>
  <!-- System Monitoring Summary -->
  <div class="glass-card p-6 flex flex-col">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-5 h-5 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
          </svg>
        </div>
        <div>
          <h2 class="text-lg font-semibold text-white">Monitoring</h2>
          <p class="text-xs text-white/60">System resources &amp; health</p>
        </div>
      </div>
      <RouterLink to="/dashboard/monitoring" class="web5-card-actions-top glass-button glass-button-sm px-3 rounded-lg text-sm font-medium items-center gap-2 no-underline">
        Details
      </RouterLink>
    </div>

    <div class="space-y-3">
      <!-- CPU -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <span class="text-sm text-white/80">CPU</span>
        </div>
        <div class="flex items-center gap-3">
          <div class="w-24 h-1.5 bg-white/10 rounded-full overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :class="barColor(cpuPercent)" :style="{ width: cpuPercent + '%' }" />
          </div>
          <span class="text-sm font-medium text-white min-w-[3rem] text-right">{{ cpuPercent.toFixed(0) }}%</span>
        </div>
      </div>

      <!-- Memory -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <span class="text-sm text-white/80">Memory</span>
        </div>
        <div class="flex items-center gap-3">
          <div class="w-24 h-1.5 bg-white/10 rounded-full overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :class="barColor(memPercent)" :style="{ width: memPercent + '%' }" />
          </div>
          <span class="text-sm font-medium text-white min-w-[3rem] text-right">{{ memPercent.toFixed(0) }}%</span>
        </div>
      </div>

      <!-- Disk -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <span class="text-sm text-white/80">Disk</span>
        </div>
        <div class="flex items-center gap-3">
          <div class="w-24 h-1.5 bg-white/10 rounded-full overflow-hidden">
            <div class="h-full rounded-full transition-all duration-500" :class="barColor(diskPercent)" :style="{ width: diskPercent + '%' }" />
          </div>
          <span class="text-sm font-medium text-white min-w-[3rem] text-right">{{ diskPercent.toFixed(0) }}%</span>
        </div>
      </div>

      <!-- Uptime -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <span class="text-sm text-white/80">Uptime</span>
        <span class="text-sm font-medium text-white/60">{{ uptimeDisplay }}</span>
      </div>
    </div>

    <div class="mt-auto pt-6">
      <RouterLink to="/dashboard/monitoring" class="web5-card-actions-bottom mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors shrink-0 no-underline">
        Details
      </RouterLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, onActivated, onDeactivated } from 'vue'
import { RouterLink } from 'vue-router'
import { useHomeStatusStore } from '@/stores/homeStatus'

const homeStatus = useHomeStatusStore()

const cpuPercent = computed(() => homeStatus.stats.cpuPercent)
const memPercent = computed(() => homeStatus.stats.memPercent)
const diskPercent = computed(() => homeStatus.stats.diskPercent)

const uptimeDisplay = computed(() => {
  const s = homeStatus.stats.uptimeSecs
  if (s === 0) return '--'
  const days = Math.floor(s / 86400)
  const hours = Math.floor((s % 86400) / 3600)
  const mins = Math.floor((s % 3600) / 60)
  if (days > 0) return `${days}d ${hours}h`
  return `${hours}h ${mins}m`
})

function barColor(pct: number): string {
  if (pct > 85) return 'bg-red-400'
  if (pct > 60) return 'bg-orange-400'
  return 'bg-green-400'
}

async function loadStats() {
  await homeStatus.refreshSystemStats()
}

// 02-11 gap closure: this card is rendered inside Web5.vue, which joined
// KEEP_ALIVE_PATHS in 02-04 — but this poll was only ever armed in
// onMounted and disarmed in onBeforeUnmount, so once Web5.vue (and this
// card with it) is KeepAlive'd, onBeforeUnmount never fires again and the
// poll ran forever in the background regardless of which dashboard tab
// was showing. It is also a redundant second poll of the exact same
// `homeStatus` store Home.vue's own `systemStatsInterval` already
// refreshes every 10s while Home is active (02-04's own per-view table).
// Arm/disarm now follows activate/deactivate, matching the rest of this
// phase's convention; armWeb5MonitoringPoll always clears any prior timer
// first, so calling it from both onMounted and onActivated on a fresh
// KeepAlive-wrapped mount is safe.
let refreshInterval: ReturnType<typeof setInterval> | null = null

function armWeb5MonitoringPoll() {
  loadStats()
  if (refreshInterval) clearInterval(refreshInterval)
  refreshInterval = setInterval(loadStats, 30000)
}
function disarmWeb5MonitoringPoll() {
  if (refreshInterval) {
    clearInterval(refreshInterval)
    refreshInterval = null
  }
}

onMounted(() => {
  armWeb5MonitoringPoll()
})

onActivated(() => {
  armWeb5MonitoringPoll()
})

onDeactivated(() => {
  disarmWeb5MonitoringPoll()
})

onBeforeUnmount(() => {
  disarmWeb5MonitoringPoll()
})
</script>
