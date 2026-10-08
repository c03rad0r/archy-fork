<template>
  <div class="glass-card p-5 mb-6">
    <h3 class="text-sm font-medium text-white/80 mb-4">Fleet Alerts</h3>

    <p v-if="errorMessage" role="status" class="text-sm text-amber-200 mb-3">{{ errorMessage }}</p>
    <div v-if="alertsLoading" class="text-white/40 text-sm py-4 text-center">
      Loading alerts...
    </div>
    <div v-else-if="!alerts.length && !errorMessage" class="text-white/40 text-sm py-4 text-center">
      No alerts across the fleet.
    </div>
    <div v-else class="space-y-2 max-h-80 overflow-y-auto">
      <div
        v-for="(alert, idx) in alerts.slice(0, 50)"
        :key="idx"
        class="flex items-start gap-3 p-3 bg-white/5 rounded-lg"
      >
        <span
          class="inline-block w-2 h-2 rounded-full mt-1.5 flex-shrink-0"
          :class="alertSeverityDot(alert.rule)"
        ></span>
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2 mb-0.5">
            <span class="fleet-node-badge">{{ alert.node_id.slice(0, 8) }}</span>
            <span class="text-xs text-white/40">{{ alertTypeLabel(alert.rule) }}</span>
          </div>
          <p class="text-sm text-white/80">{{ alert.message }}</p>
          <p class="text-xs text-white/30 mt-0.5">{{ formatTimestamp(alert.timestamp) }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { type FleetAlert, alertSeverityDot, alertTypeLabel, formatTimestamp } from './useFleetData'

defineProps<{
  alerts: FleetAlert[]
  alertsLoading: boolean
  errorMessage?: string
}>()
</script>
