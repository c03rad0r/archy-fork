<template>
  <div v-if="false" data-controller-container tabindex="0" class="glass-card p-6 mb-6 transition-all hover:-translate-y-1">
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
      <!-- Service Status -->
      <div class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="servicesRunning ? 'bg-green-400' : 'bg-red-400'"></div>
            <div v-if="servicesRunning" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">Services</p>
            <p class="text-xs text-white/60">{{ servicesRunning ? 'All Running' : 'Some Stopped' }}</p>
          </div>
        </div>
        <button
          @click="$emit('restartServices')"
          class="w-full min-h-[44px] glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50 flex items-center justify-center"
          :disabled="restarting"
        >
          {{ restarting ? 'Restarting...' : 'Restart' }}
        </button>
      </div>

      <!-- Tor Status -->
      <div class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
        <div class="flex items-center gap-3 min-w-0">
          <div class="relative shrink-0">
            <div class="w-3 h-3 rounded-full" :class="torStatusColor"></div>
            <div v-if="torStatusLabel === 'running'" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
          </div>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">Tor Status</p>
            <p class="text-xs text-white/60 capitalize">{{ torStatusLabel }}</p>
          </div>
        </div>
        <button
          @click="$emit('checkTor')"
          class="w-full min-h-[44px] glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50 flex items-center justify-center"
          :disabled="checkingTor"
        >
          {{ checkingTor ? 'Checking...' : 'Check Tor' }}
        </button>
      </div>

      <!-- Auto-Sync Toggle (hidden — not functional yet) -->
      <div v-if="false" class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
        <div class="flex items-center justify-between min-w-0">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-white/60 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z" />
            </svg>
            <div class="min-w-0">
              <p class="text-sm font-medium text-white">Auto-Sync</p>
              <p class="text-xs text-white/60">{{ autoSyncEnabled ? 'Enabled' : 'Disabled' }}</p>
            </div>
          </div>
          <ToggleSwitch :model-value="autoSyncEnabled" @update:model-value="$emit('update:autoSyncEnabled', $event)" />
        </div>
      </div>

      <!-- Logs & Diagnostics -->
      <div class="flex flex-col gap-3 p-4 bg-white/5 rounded-lg min-w-0">
        <div class="flex items-center gap-3 min-w-0">
          <svg class="w-5 h-5 text-white/60 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <div class="min-w-0">
            <p class="text-sm font-medium text-white">Logs</p>
            <p class="text-xs text-white/60">{{ logCount > 0 ? `${logCount} new` : 'No new logs' }}</p>
          </div>
        </div>
        <button
          @click="$emit('viewLogs')"
          class="w-full min-h-[44px] glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors flex items-center justify-center"
        >
          View
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import ToggleSwitch from '@/components/ToggleSwitch.vue'

defineProps<{
  servicesRunning: boolean
  restarting: boolean
  torStatusLabel: string
  torStatusColor: string
  checkingTor: boolean
  autoSyncEnabled: boolean
  logCount: number
}>()

defineEmits<{
  restartServices: []
  checkTor: []
  'update:autoSyncEnabled': [value: boolean]
  viewLogs: []
}>()
</script>
