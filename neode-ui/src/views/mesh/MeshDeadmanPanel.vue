<script setup lang="ts">
import { ref } from 'vue'
import { useMeshStore } from '@/stores/mesh'
import ToggleSwitch from '@/components/ToggleSwitch.vue'

const mesh = useMeshStore()

const deadmanConfiguring = ref(false)
const deadmanInterval = ref('21600')
const deadmanEnabled = ref(false)
const deadmanCustomMsg = ref('')

// Sync from store on creation
if (mesh.deadmanStatus) {
  deadmanEnabled.value = mesh.deadmanStatus.dead_man_enabled
  deadmanInterval.value = String(mesh.deadmanStatus.dead_man_interval_secs)
}

function formatTimeRemaining(secs: number): string {
  if (secs >= 86400) return `${Math.floor(secs / 3600)}h`
  if (secs >= 3600) return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`
  if (secs >= 60) return `${Math.floor(secs / 60)}m ${secs % 60}s`
  return `${secs}s`
}

async function handleDeadmanToggle() {
  deadmanConfiguring.value = true
  try {
    await mesh.configureDeadman({ enabled: deadmanEnabled.value })
    await mesh.fetchDeadmanStatus()
  } finally {
    deadmanConfiguring.value = false
  }
}

async function handleDeadmanConfigure() {
  deadmanConfiguring.value = true
  try {
    await mesh.configureDeadman({
      enabled: deadmanEnabled.value,
      interval_secs: parseInt(deadmanInterval.value) || 21600,
      custom_message: deadmanCustomMsg.value || undefined,
    })
    await mesh.fetchDeadmanStatus()
  } finally {
    deadmanConfiguring.value = false
  }
}

async function handleDeadmanCheckin() {
  await mesh.deadmanCheckin()
  await mesh.fetchDeadmanStatus()
}
</script>

<template>
  <div class="glass-card mesh-deadman-panel">
    <h3 class="mesh-panel-title">Dead Man's Switch</h3>
    <p class="mesh-panel-sub">Auto-broadcasts a signed emergency alert if you don't check in</p>

    <!-- Status -->
    <div v-if="mesh.deadmanStatus" class="mesh-deadman-status">
      <div class="mesh-deadman-indicator" :class="mesh.deadmanStatus.triggered ? 'triggered' : mesh.deadmanStatus.dead_man_enabled ? 'armed' : 'disabled'">
        {{ mesh.deadmanStatus.triggered ? 'TRIGGERED' : mesh.deadmanStatus.dead_man_enabled ? 'ARMED' : 'DISABLED' }}
      </div>
      <div v-if="mesh.deadmanStatus.dead_man_enabled && !mesh.deadmanStatus.triggered" class="mesh-deadman-timer">
        {{ formatTimeRemaining(mesh.deadmanStatus.time_remaining_secs) }}
      </div>
      <div v-if="deadmanCustomMsg || mesh.deadmanStatus.dead_man_enabled" class="mesh-deadman-message">
        {{ deadmanCustomMsg || 'Dead man\'s switch triggered — operator unresponsive' }}
      </div>
      <button v-if="mesh.deadmanStatus.dead_man_enabled" class="glass-button mesh-deadman-checkin-btn" @click="handleDeadmanCheckin">
        I'm OK — Check In
      </button>
    </div>

    <!-- Configuration -->
    <div class="mesh-deadman-config">
      <button
        @click="deadmanEnabled = !deadmanEnabled; handleDeadmanToggle()"
        class="w-full flex items-center gap-4 p-4 rounded-xl border transition-all text-left mb-3"
        :class="deadmanEnabled
          ? 'bg-white/10 border-orange-500/40'
          : 'bg-black/20 border-white/10 hover:border-white/20'"
      >
        <svg class="w-5 h-5 shrink-0" :class="deadmanEnabled ? 'text-orange-400' : 'text-white/40'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4.5c-.77-.833-2.694-.833-3.464 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
        </svg>
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium" :class="deadmanEnabled ? 'text-white/95' : 'text-white/70'">{{ deadmanEnabled ? 'Dead Man\'s Switch Active' : 'Enable Dead Man\'s Switch' }}</p>
          <p class="text-xs text-white/50 mt-0.5">Auto-alerts your contacts if you don't check in</p>
        </div>
        <ToggleSwitch :model-value="deadmanEnabled" @click.stop @update:model-value="deadmanEnabled = $event; handleDeadmanToggle()" />
      </button>

      <template v-if="deadmanEnabled">
        <div class="mesh-deadman-field">
          <label class="mesh-bitcoin-label">Trigger Interval</label>
          <select v-model="deadmanInterval" class="mesh-bitcoin-input mesh-bitcoin-input-sm">
            <option value="3600">1 hour</option>
            <option value="21600">6 hours</option>
            <option value="43200">12 hours</option>
            <option value="86400">24 hours</option>
          </select>
        </div>

        <div class="mesh-deadman-field">
          <label class="mesh-bitcoin-label">Alert Message</label>
          <input v-model="deadmanCustomMsg" class="mesh-bitcoin-input" placeholder="Dead man's switch triggered — operator unresponsive" />
        </div>

        <div class="mesh-deadman-info">
          <span v-if="mesh.deadmanStatus?.has_gps" class="mesh-deadman-info-item">GPS: included</span>
          <span class="mesh-deadman-info-item">Contacts: {{ mesh.deadmanStatus?.emergency_contacts ?? 0 }}</span>
        </div>

        <button class="glass-button" :disabled="deadmanConfiguring" @click="handleDeadmanConfigure">
          {{ deadmanConfiguring ? 'Saving...' : 'Save Configuration' }}
        </button>
      </template>
    </div>
  </div>
</template>

<style>
.mesh-deadman-panel { padding: 18px; display: flex; flex-direction: column; gap: 14px; flex: 1; min-height: 0; overflow-y: auto; }
.mesh-deadman-status { display: flex; flex-direction: column; gap: 8px; align-items: center; padding: 16px; background: rgba(0,0,0,0.2); border-radius: 10px; }
.mesh-deadman-indicator { font-size: 0.75rem; font-weight: 700; letter-spacing: 1px; padding: 4px 14px; border-radius: 6px; text-transform: uppercase; }
.mesh-deadman-indicator.armed { background: rgba(251,146,60,0.15); color: #fb923c; border: 1px solid rgba(251,146,60,0.3); }
.mesh-deadman-indicator.disabled { background: rgba(255,255,255,0.06); color: rgba(255,255,255,0.4); border: 1px solid rgba(255,255,255,0.1); }
.mesh-deadman-indicator.triggered { background: rgba(239,68,68,0.15); color: #ef4444; border: 1px solid rgba(239,68,68,0.3); animation: pulse-alert 1.5s infinite; }
@keyframes pulse-alert { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }
.mesh-deadman-timer { font-size: 1.6rem; font-weight: 700; color: #fb923c; font-family: monospace; }
.mesh-deadman-message { font-size: 0.78rem; color: rgba(255,255,255,0.4); text-align: center; }
.mesh-deadman-checkin-btn { margin-top: 4px; }
.mesh-deadman-config { display: flex; flex-direction: column; gap: 10px; }
.mesh-deadman-field { display: flex; flex-direction: column; gap: 4px; }
.mesh-deadman-info { display: flex; gap: 12px; flex-wrap: wrap; }
.mesh-deadman-info-item { font-size: 0.75rem; color: rgba(255,255,255,0.4); background: rgba(255,255,255,0.05); padding: 3px 10px; border-radius: 4px; }
</style>
