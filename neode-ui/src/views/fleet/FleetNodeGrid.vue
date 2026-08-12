<template>
  <div class="glass-card p-5 mb-6">
    <div class="flex items-center justify-between mb-4">
      <h3 class="text-sm font-medium text-white/80">Nodes</h3>
      <div class="flex gap-2">
        <button
          v-for="opt in SORT_OPTIONS"
          :key="opt.value"
          class="fleet-sort-btn"
          :class="{ 'fleet-sort-btn-active': sortBy === opt.value }"
          @click="$emit('update:sortBy', opt.value)"
        >
          {{ opt.label }}
        </button>
      </div>
    </div>

    <!-- Empty State -->
    <div v-if="!nodes.length" class="text-white/40 text-sm py-8 text-center">
      No nodes reporting. Ensure telemetry is enabled on beta nodes.
    </div>

    <div v-else class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
      <div
        v-for="node in sortedNodes"
        :key="node.node_id"
        class="fleet-node-card"
        :class="{ 'fleet-node-card-selected': selectedNodeId === node.node_id }"
        @click="$emit('selectNode', node.node_id)"
      >
        <div class="flex items-center justify-between mb-3">
          <div class="flex items-center gap-2">
            <span
              class="fleet-status-dot"
              :class="isOnline(node.reported_at) ? 'fleet-dot-online' : 'fleet-dot-offline'"
            ></span>
            <span class="text-sm font-semibold text-white truncate">{{ fleetNodeDisplayName(node) }}</span>
          </div>
          <span class="fleet-version-badge">{{ $ver(node.version) }}</span>
        </div>
        <div class="mb-3 truncate text-xs text-white/40">
          {{ fleetNodeSubtitle(node) }}
        </div>

        <div class="space-y-2 mb-3">
          <div class="fleet-metric-row">
            <span class="text-xs text-white/50">CPU</span>
            <div class="fleet-bar-track">
              <div
                class="fleet-bar-fill"
                :class="healthBarClass(node.cpu_pct)"
                :style="{ width: Math.min(node.cpu_pct, 100) + '%' }"
              ></div>
            </div>
            <span class="text-xs text-white/60 w-10 text-right">{{ node.cpu_pct.toFixed(0) }}%</span>
          </div>
          <div class="fleet-metric-row">
            <span class="text-xs text-white/50">RAM</span>
            <div class="fleet-bar-track">
              <div
                class="fleet-bar-fill"
                :class="healthBarClass(node.mem_pct)"
                :style="{ width: Math.min(node.mem_pct, 100) + '%' }"
              ></div>
            </div>
            <span class="text-xs text-white/60 w-10 text-right">{{ node.mem_pct.toFixed(0) }}%</span>
          </div>
          <div class="fleet-metric-row">
            <span class="text-xs text-white/50">Disk</span>
            <div class="fleet-bar-track">
              <div
                class="fleet-bar-fill"
                :class="healthBarClass(node.disk_pct)"
                :style="{ width: Math.min(node.disk_pct, 100) + '%' }"
              ></div>
            </div>
            <span class="text-xs text-white/60 w-10 text-right">{{ node.disk_pct.toFixed(0) }}%</span>
          </div>
        </div>

        <div class="flex items-center justify-between text-xs text-white/40">
          <span>{{ node.running_count }}/{{ node.container_count }} containers</span>
          <span>{{ node.federation_peers }} peers</span>
        </div>
        <div class="flex items-center justify-between text-xs text-white/40 mt-1">
          <span>Up {{ formatUptime(node.uptime_secs) }}</span>
          <span>{{ timeAgo(node.reported_at) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  type FleetNode, type SortOption, SORT_OPTIONS,
  isOnline, healthBarClass, formatUptime, timeAgo, fleetNodeDisplayName, fleetNodeSubtitle,
} from './useFleetData'

defineProps<{
  nodes: FleetNode[]
  sortedNodes: FleetNode[]
  sortBy: SortOption
  selectedNodeId: string | null
}>()

defineEmits<{
  'update:sortBy': [value: SortOption]
  selectNode: [nodeId: string]
}>()
</script>
