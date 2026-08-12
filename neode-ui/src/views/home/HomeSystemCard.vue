<template>
  <div
    data-controller-container
    tabindex="0"
    class="home-card controller-focusable lg:col-span-2"
    :class="{ 'home-card-animate': animate }"
    style="--card-stagger: 6"
  >
    <div class="home-card-shell">
      <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
        <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
          <div class="home-card-text">
            <h2 class="text-xl font-semibold text-white mb-1">{{ t('home.system') }}</h2>
            <p class="text-sm text-white/70">{{ uptimeDisplay }}</p>
          </div>
          <RouterLink to="/dashboard/monitoring?from=home" :aria-label="t('home.goToSettings')" class="text-white/60 hover:text-white transition-colors">
            <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </RouterLink>
        </div>
        <div class="home-card-stats grid grid-cols-1 gap-4 flex-1 min-h-0" :class="stats.bitcoinAvailable ? 'sm:grid-cols-4' : 'sm:grid-cols-3'">
          <template v-if="!loaded">
            <div v-for="i in 3" :key="i" class="p-4 bg-white/5 rounded-lg animate-pulse">
              <div class="flex items-center justify-between mb-2">
                <div class="w-8 h-3 bg-white/10 rounded"></div>
                <div class="w-12 h-4 bg-white/10 rounded"></div>
              </div>
              <div class="w-full h-2 bg-white/10 rounded-full"></div>
            </div>
          </template>
          <template v-else>
            <div class="p-4 bg-white/5 rounded-lg">
              <div class="flex items-center justify-between mb-2">
                <p class="text-xs text-white/60">{{ t('home.cpu') }}</p>
                <p class="text-sm font-medium" :class="gaugeTextColor(stats.cpuPercent)">{{ (stats.cpuPercent ?? 0).toFixed(0) }}%</p>
              </div>
              <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
                <div class="h-full rounded-full transition-all duration-500" :class="gaugeBarColor(stats.cpuPercent)" :style="{ width: stats.cpuPercent + '%' }"></div>
              </div>
            </div>
            <div class="p-4 bg-white/5 rounded-lg">
              <div class="flex items-center justify-between mb-2">
                <p class="text-xs text-white/60">{{ t('home.ram') }}</p>
                <p class="text-sm font-medium" :class="gaugeTextColor(stats.memPercent)">{{ formatBytes(stats.memUsed) }} / {{ formatBytes(stats.memTotal) }}</p>
              </div>
              <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
                <div class="h-full rounded-full transition-all duration-500" :class="gaugeBarColor(stats.memPercent)" :style="{ width: stats.memPercent + '%' }"></div>
              </div>
            </div>
            <div class="p-4 bg-white/5 rounded-lg">
              <div class="flex items-center justify-between mb-2">
                <p class="text-xs text-white/60">{{ t('home.disk') }}</p>
                <p class="text-sm font-medium" :class="gaugeTextColor(stats.diskPercent)">{{ formatBytes(stats.diskUsed) }} / {{ formatBytes(stats.diskTotal) }}</p>
              </div>
              <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
                <div class="h-full rounded-full transition-all duration-500" :class="gaugeBarColor(stats.diskPercent)" :style="{ width: stats.diskPercent + '%' }"></div>
              </div>
            </div>
            <div v-if="stats.bitcoinAvailable" class="p-4 bg-white/5 rounded-lg">
              <div class="flex items-center justify-between mb-2">
                <p class="text-xs text-orange-400/80">Bitcoin</p>
                <p class="text-sm font-medium" :class="stats.bitcoinStale ? 'text-white/50' : (stats.bitcoinSyncPercent >= 99.9 ? 'text-green-400' : 'text-orange-400')">
                  {{ stats.bitcoinStale ? 'Updating…' : (stats.bitcoinSyncPercent >= 99.9 ? 'Synced' : stats.bitcoinSyncPercent.toFixed(1) + '%') }}
                </p>
              </div>
              <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden">
                <div class="h-full rounded-full transition-all duration-500" :class="stats.bitcoinSyncPercent >= 99.9 ? 'bg-green-400' : 'bg-orange-400'" :style="{ width: Math.min(stats.bitcoinSyncPercent, 100) + '%' }"></div>
              </div>
              <p class="text-xs text-white/40 mt-1">Block {{ stats.bitcoinBlockHeight.toLocaleString() }}</p>
            </div>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

defineProps<{
  animate: boolean
  loaded: boolean
  stats: {
    cpuPercent: number
    memUsed: number
    memTotal: number
    memPercent: number
    diskUsed: number
    diskTotal: number
    diskPercent: number
    bitcoinSyncPercent: number
    bitcoinBlockHeight: number
    bitcoinAvailable: boolean
    bitcoinStale?: boolean
  }
  uptimeDisplay: string
}>()

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  const val = bytes / Math.pow(1024, i)
  return `${val < 10 ? val.toFixed(1) : Math.round(val)} ${units[i]}`
}

function gaugeTextColor(pct: number): string {
  if (pct >= 90) return 'text-red-400'
  if (pct >= 70) return 'text-orange-400'
  return 'text-green-400'
}

function gaugeBarColor(pct: number): string {
  if (pct >= 90) return 'bg-red-400'
  if (pct >= 70) return 'bg-orange-400'
  return 'bg-green-400'
}
</script>
