<script setup lang="ts">
import { ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'

const telemetryEnabled = ref(false)
const telemetryLoading = ref(false)

async function loadTelemetryStatus() {
  try {
    const res = await rpcClient.call<{ enabled: boolean }>({ method: 'analytics.get-status' })
    telemetryEnabled.value = res.enabled
  } catch { /* ignore */ }
}

async function toggleTelemetry() {
  telemetryLoading.value = true
  try {
    const method = telemetryEnabled.value ? 'analytics.disable' : 'analytics.enable'
    await rpcClient.call({ method })
    telemetryEnabled.value = !telemetryEnabled.value
  } catch { /* ignore */ }
  telemetryLoading.value = false
}

loadTelemetryStatus()
</script>

<template>
  <!-- Beta Telemetry Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-center justify-between mb-3">
      <div>
        <h2 class="text-xl font-semibold text-white/96 mb-1">Beta Telemetry</h2>
        <p class="text-sm text-white/60">Help improve Archipelago by sharing anonymous system health data. No wallet data, no keys, no personal info.</p>
      </div>
      <button
        @click="toggleTelemetry"
        :disabled="telemetryLoading"
        class="shrink-0 ml-4 px-4 py-2 rounded-lg text-sm font-medium transition-colors"
        :class="telemetryEnabled ? 'glass-button glass-button-success' : 'glass-button'"
      >
        {{ telemetryLoading ? '...' : telemetryEnabled ? 'Enabled' : 'Enable' }}
      </button>
    </div>
    <div v-if="telemetryEnabled" class="mt-3 text-xs text-white/50 space-y-1">
      <p>Reporting: version, uptime, container states, CPU/RAM, error alerts.</p>
      <p>Not reporting: wallet balances, private keys, DIDs, IP addresses.</p>
    </div>
  </div>
</template>
