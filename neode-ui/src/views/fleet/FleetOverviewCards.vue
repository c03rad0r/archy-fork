<template>
  <div class="grid grid-cols-2 lg:grid-cols-5 gap-4 mb-6">
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Total Nodes</p>
      <p class="text-2xl font-bold text-white">{{ nodeCount }}</p>
      <p class="text-xs text-white/40">
        <span class="fleet-dot-online"></span> {{ onlineCount }} online
        <span class="ml-1 fleet-dot-offline"></span> {{ offlineCount }} not reporting
        <span v-if="unknownCount" class="ml-1">{{ unknownCount }} unknown</span>
      </p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Fleet Health</p>
      <p class="text-2xl font-bold text-white">{{ fleetHealthPct }}%</p>
      <p class="text-xs text-white/40">{{ healthyCount }}/{{ nodeCount }} reporting without alerts</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg CPU</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgCpu)">{{ formatMetric(avgCpu, 1) }}</p>
      <p class="text-xs text-white/40">reporting online nodes</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg RAM</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgMem)">{{ formatMetric(avgMem, 1) }}</p>
      <p class="text-xs text-white/40">reporting online nodes</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg Disk</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgDisk)">{{ formatMetric(avgDisk, 1) }}</p>
      <p class="text-xs text-white/40">reporting online nodes</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { healthTextClass, formatMetric } from './useFleetData'

defineProps<{
  nodeCount: number
  onlineCount: number
  offlineCount: number
  unknownCount: number
  fleetHealthPct: number
  healthyCount: number
  avgCpu: number | null
  avgMem: number | null
  avgDisk: number | null
}>()
</script>
