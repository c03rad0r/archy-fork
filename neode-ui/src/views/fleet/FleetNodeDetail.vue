<template>
  <div v-if="node" class="glass-card p-5 mb-6">
    <div class="flex items-center justify-between mb-4">
      <h3 class="text-sm font-medium text-white/80">
        Node Detail — <span>{{ fleetNodeDisplayName(node) }}</span>
      </h3>
      <button class="glass-button text-xs px-3 py-1" @click="$emit('close')">Close</button>
    </div>

    <!-- Node Info Summary -->
    <div class="grid grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">Hostname</p>
        <p class="text-lg font-bold text-white truncate">{{ node.hostname || fleetNodeDisplayName(node) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">Address</p>
        <p class="text-lg font-bold text-white truncate">{{ node.server_url || nodeId.slice(0, 8) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">Version</p>
        <p class="text-lg font-bold text-white">{{ $ver(node.version) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">Uptime</p>
        <p class="text-lg font-bold text-white">{{ formatUptime(node.uptime_secs) }}</p>
      </div>
      <div class="monitoring-stat-card">
        <p class="text-xs text-white/50 uppercase tracking-wide">Federation Peers</p>
        <p class="text-lg font-bold text-white">{{ node.federation_peers }}</p>
      </div>
    </div>

    <!-- History Charts -->
    <p v-if="historyMessage" role="status" class="text-sm text-amber-200 mb-4">{{ historyMessage }}</p>
    <div v-if="historyLoading && !historyLabels.length" class="text-white/40 text-sm py-4 text-center mb-4">
      Loading history...
    </div>
    <div v-else-if="historyLoading" class="text-white/40 text-xs text-center mb-4">
      Refreshing history...
    </div>
    <div v-else-if="historyLabels.length" class="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
      <div class="glass-card p-4">
        <h4 class="text-xs font-medium text-white/60 mb-2">CPU History</h4>
        <LineChart
          :datasets="cpuDatasets"
          :labels="historyLabels"
          :width="chartWidth"
          :height="160"
          :y-max="100"
        />
      </div>
      <div class="glass-card p-4">
        <h4 class="text-xs font-medium text-white/60 mb-2">RAM History</h4>
        <LineChart
          :datasets="memDatasets"
          :labels="historyLabels"
          :width="chartWidth"
          :height="160"
          :y-max="100"
        />
      </div>
      <div class="glass-card p-4">
        <h4 class="text-xs font-medium text-white/60 mb-2">Disk History</h4>
        <LineChart
          :datasets="diskDatasets"
          :labels="historyLabels"
          :width="chartWidth"
          :height="160"
          :y-max="100"
        />
      </div>
    </div>

    <!-- Container List -->
    <div class="mb-4">
      <h4 class="text-xs font-medium text-white/60 mb-2 uppercase tracking-wide">Containers</h4>
      <div v-if="!node.containers?.length" class="text-white/40 text-sm py-2">
        No containers reported.
      </div>
      <div v-else class="space-y-1">
        <div
          v-for="c in (node.containers || [])"
          :key="c.id"
          class="flex items-center gap-3 p-2 bg-white/5 rounded-lg"
        >
          <span
            class="inline-block w-2 h-2 rounded-full flex-shrink-0"
            :class="c.state === 'running' ? 'bg-green-400' : 'bg-red-400'"
          ></span>
          <span class="text-sm text-white flex-1 truncate">{{ c.id }}</span>
          <span class="text-xs text-white/40">{{ c.state }}</span>
          <span class="text-xs text-white/30">{{ c.version }}</span>
        </div>
      </div>
    </div>

    <!-- Node Alerts -->
    <div>
      <h4 class="text-xs font-medium text-white/60 mb-2 uppercase tracking-wide">Recent Alerts</h4>
      <div v-if="!node.recent_alerts.length" class="text-white/40 text-sm py-2">
        No recent alerts for this node.
      </div>
      <div v-else class="space-y-1">
        <div
          v-for="(alert, idx) in node.recent_alerts"
          :key="idx"
          class="flex items-start gap-3 p-2 bg-white/5 rounded-lg"
        >
          <span
            class="inline-block w-2 h-2 rounded-full mt-1.5 flex-shrink-0"
            :class="alertSeverityDot(alert.rule)"
          ></span>
          <div class="flex-1 min-w-0">
            <p class="text-sm text-white/80">{{ alert.message }}</p>
            <p class="text-xs text-white/30">{{ formatTimestamp(alert.timestamp) }}</p>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import LineChart from '@/components/LineChart.vue'
import type { ChartDataset } from '@/components/LineChart.vue'
import { type FleetNode, formatUptime, alertSeverityDot, formatTimestamp, fleetNodeDisplayName } from './useFleetData'

defineProps<{
  node: FleetNode | null
  nodeId: string
  historyLoading: boolean
  historyMessage?: string
  historyLabels: string[]
  cpuDatasets: ChartDataset[]
  memDatasets: ChartDataset[]
  diskDatasets: ChartDataset[]
  chartWidth: number
}>()

defineEmits<{
  close: []
}>()
</script>
