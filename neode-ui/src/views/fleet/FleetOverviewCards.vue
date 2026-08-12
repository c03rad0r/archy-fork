<template>
  <div class="grid grid-cols-2 lg:grid-cols-5 gap-4 mb-6">
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Total Nodes</p>
      <p class="text-2xl font-bold text-white">{{ nodeCount }}</p>
      <p class="text-xs text-white/40">
        <span class="fleet-dot-online"></span> {{ onlineCount }} online
        <span class="ml-1 fleet-dot-offline"></span> {{ offlineCount }} offline
      </p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Fleet Health</p>
      <p class="text-2xl font-bold text-white">{{ fleetHealthPct }}%</p>
      <p class="text-xs text-white/40">{{ healthyCount }}/{{ nodeCount }} no alerts</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg CPU</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgCpu)">{{ avgCpu.toFixed(1) }}%</p>
      <p class="text-xs text-white/40">across fleet</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg RAM</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgMem)">{{ avgMem.toFixed(1) }}%</p>
      <p class="text-xs text-white/40">across fleet</p>
    </div>
    <div data-controller-container tabindex="0" class="monitoring-stat-card">
      <p class="text-xs text-white/50 uppercase tracking-wide">Avg Disk</p>
      <p class="text-2xl font-bold text-white" :class="healthTextClass(avgDisk)">{{ avgDisk.toFixed(1) }}%</p>
      <p class="text-xs text-white/40">across fleet</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { healthTextClass } from './useFleetData'

defineProps<{
  nodeCount: number
  onlineCount: number
  offlineCount: number
  fleetHealthPct: number
  healthyCount: number
  avgCpu: number
  avgMem: number
  avgDisk: number
}>()
</script>
