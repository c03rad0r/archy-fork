<template>
  <div class="pb-16 md:pb-6 mobile-scroll-pad">
    <BackButton label="Web5" desktop-margin="mb-6" @click="router.push('/dashboard/web5')" />

    <!-- Header -->
    <div class="hidden md:block mb-8">
      <div class="flex flex-col gap-4 min-[1440px]:flex-row min-[1440px]:items-center min-[1440px]:justify-between">
        <div>
          <h1 class="text-3xl font-bold text-white mb-2">Fleet Dashboard</h1>
          <p class="text-white/70">Beta Telemetry — monitoring {{ fleet.nodes.value.length }} node{{ fleet.nodes.value.length !== 1 ? 's' : '' }}</p>
        </div>
        <div class="grid grid-cols-3 gap-2 min-[1440px]:flex min-[1440px]:items-center">
          <div class="monitoring-stat-card monitoring-stat-card-compact col-span-3 flex h-10 min-h-10 items-center justify-between gap-3 whitespace-nowrap min-[1440px]:col-span-1 min-[1440px]:min-w-[220px]">
            <p class="text-[11px] font-medium uppercase tracking-wide text-white/50">Auto Refresh</p>
            <div class="flex min-w-0 items-center gap-2 whitespace-nowrap">
              <span
                class="inline-block h-1.5 w-1.5 rounded-full"
                :class="fleet.autoRefresh.value ? 'bg-emerald-300' : 'bg-white/35'"
              ></span>
              <p class="text-sm font-bold text-white">{{ fleet.autoRefresh.value ? '60s' : 'Paused' }}</p>
            </div>
          </div>
          <button class="glass-button text-sm px-4 py-2" @click="fleet.toggleAutoRefresh">
            {{ fleet.autoRefresh.value ? 'Pause' : 'Resume' }}
          </button>
          <button class="glass-button text-sm px-4 py-2 disabled:opacity-50" :disabled="fleet.refreshing.value" @click="fleet.refreshAll">
            {{ fleet.refreshing.value ? 'Refreshing...' : 'Refresh' }}
          </button>
          <button class="glass-button text-sm px-4 py-2" @click="fleet.exportFleetData">
            Export JSON
          </button>
        </div>
      </div>
    </div>

    <!-- Mobile Header -->
    <div class="md:hidden mb-6">
      <h1 class="text-2xl font-bold text-white mb-1">Fleet Dashboard</h1>
      <p class="text-white/60 text-sm mb-3">Monitoring {{ fleet.nodes.value.length }} node{{ fleet.nodes.value.length !== 1 ? 's' : '' }}</p>
      <div class="monitoring-stat-card monitoring-stat-card-compact mb-3 flex h-10 min-h-10 items-center justify-between gap-3 whitespace-nowrap">
        <p class="text-[11px] font-medium uppercase tracking-wide text-white/50">Auto Refresh</p>
        <div class="flex min-w-0 items-center gap-2 whitespace-nowrap">
          <span
            class="inline-block h-1.5 w-1.5 rounded-full"
            :class="fleet.autoRefresh.value ? 'bg-emerald-300' : 'bg-white/35'"
          ></span>
          <p class="text-sm font-bold text-white">{{ fleet.autoRefresh.value ? '60s' : 'Paused' }}</p>
        </div>
      </div>
      <div class="flex gap-2">
        <button class="glass-button text-xs px-3 py-2 flex-1" @click="fleet.toggleAutoRefresh">
          {{ fleet.autoRefresh.value ? 'Pause' : 'Resume' }}
        </button>
        <button class="glass-button text-xs px-3 py-2 flex-1 disabled:opacity-50" :disabled="fleet.refreshing.value" @click="fleet.refreshAll">
          {{ fleet.refreshing.value ? 'Refreshing...' : 'Refresh' }}
        </button>
        <button class="glass-button text-xs px-3 py-2 flex-1" @click="fleet.exportFleetData">Export</button>
      </div>
    </div>

    <!-- Loading State -->
    <div v-if="fleet.loading.value" class="flex items-center justify-center py-20">
      <div class="glass-card p-8 max-w-md text-center">
        <svg class="animate-spin h-8 w-8 mx-auto mb-4 text-white/70" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <h3 class="text-lg font-semibold text-white mb-2">Loading fleet data</h3>
        <p class="text-white/60 text-sm">Checking beta telemetry reports from connected nodes.</p>
      </div>
    </div>

    <!-- Error State -->
    <div v-else-if="fleet.errorMessage.value && !fleet.nodes.value.length" class="glass-card p-6 mb-6">
      <div class="alert-error rounded-lg mb-4">{{ fleet.errorMessage.value }}</div>
      <button class="glass-button text-sm px-4 py-2" @click="fleet.refreshAll">Retry</button>
    </div>

    <!-- Dashboard Content -->
    <template v-else>
      <div v-if="fleet.errorMessage.value" role="status" class="glass-card p-4 mb-4 flex flex-wrap items-center gap-3">
        <p class="text-sm text-white/80 flex-1 min-w-0">{{ fleet.errorMessage.value }}</p>
        <button class="glass-button text-sm px-4 py-2 disabled:opacity-50" :disabled="fleet.refreshing.value" @click="fleet.refreshAll">
          {{ fleet.refreshing.value ? 'Retrying...' : 'Retry' }}
        </button>
      </div>
      <FleetOverviewCards
        :node-count="fleet.nodes.value.length"
        :online-count="fleet.onlineCount.value"
        :offline-count="fleet.offlineCount.value"
        :unknown-count="fleet.unknownCount.value"
        :fleet-health-pct="fleet.fleetHealthPct.value"
        :healthy-count="fleet.healthyCount.value"
        :avg-cpu="fleet.avgCpu.value"
        :avg-mem="fleet.avgMem.value"
        :avg-disk="fleet.avgDisk.value"
      />

      <div v-if="fleet.refreshing.value && fleet.nodes.value.length > 0" class="glass-card p-3 mb-4 text-sm text-white/60 flex items-center justify-center gap-2">
        <svg class="animate-spin h-4 w-4 text-white/50" viewBox="0 0 24 24" fill="none">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing fleet telemetry...
      </div>

      <FleetNodeGrid
        :now="fleet.now.value"
        :nodes="fleet.nodes.value"
        :sorted-nodes="fleet.sortedNodes.value"
        :sort-by="fleet.sortBy.value"
        :selected-node-id="fleet.selectedNodeId.value"
        @update:sort-by="fleet.sortBy.value = $event"
        @select-node="fleet.selectNode"
      />

      <FleetAlerts
        :alerts="fleet.fleetAlerts.value"
        :alerts-loading="fleet.alertsLoading.value"
      />

      <FleetNodeDetail
        v-if="fleet.selectedNodeId.value && fleet.selectedNode.value"
        :node="fleet.selectedNode.value"
        :node-id="fleet.selectedNodeId.value"
        :history-loading="fleet.nodeHistoryLoading.value"
        :history-labels="fleet.nodeHistoryLabels.value"
        :cpu-datasets="fleet.nodeHistoryCpuDatasets.value"
        :mem-datasets="fleet.nodeHistoryMemDatasets.value"
        :disk-datasets="fleet.nodeHistoryDiskDatasets.value"
        :chart-width="fleet.chartWidth.value"
        @close="fleet.selectedNodeId.value = null"
      />

      <FleetContainerMatrix
        :nodes="fleet.nodes.value"
        :sorted-nodes="fleet.sortedNodes.value"
        :all-app-ids="fleet.allAppIds.value"
      />

      <p class="text-xs text-white/30 mt-4 text-center">
        {{ fleet.autoRefresh.value ? 'Auto-refreshing every 60s' : 'Auto-refresh paused' }}
        &middot; Last updated {{ fleet.lastRefreshed.value ? timeAgo(fleet.lastRefreshed.value, fleet.now.value) : 'never' }}
      </p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { useRouter } from 'vue-router'
import BackButton from '@/components/BackButton.vue'
import FleetOverviewCards from './fleet/FleetOverviewCards.vue'
import FleetNodeGrid from './fleet/FleetNodeGrid.vue'
import FleetAlerts from './fleet/FleetAlerts.vue'
import FleetNodeDetail from './fleet/FleetNodeDetail.vue'
import FleetContainerMatrix from './fleet/FleetContainerMatrix.vue'
import { useFleetData, timeAgo } from './fleet/useFleetData'

const router = useRouter()

const fleet = useFleetData()
</script>
