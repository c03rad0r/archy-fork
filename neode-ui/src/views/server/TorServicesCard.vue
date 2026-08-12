<template>
  <div data-controller-container tabindex="0" class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="relative shrink-0">
          <div class="w-3 h-3 rounded-full" :class="torDaemonRunning ? 'bg-green-400' : 'bg-red-400'"></div>
          <div v-if="torDaemonRunning" class="absolute inset-0 w-3 h-3 rounded-full bg-green-400 animate-ping opacity-75"></div>
        </div>
        <div>
          <h2 class="text-xl font-semibold text-white/96">Tor Services</h2>
          <p class="text-sm text-white/60 mt-1">Manage hidden service addresses for your node and apps</p>
        </div>
      </div>
      <div class="responsive-card-actions-top items-center gap-2">
        <button @click="$emit('restartTor')" :disabled="torRestarting" class="glass-button px-3 py-2 rounded-lg text-sm">
          {{ torRestarting ? 'Restarting...' : 'Restart Tor' }}
        </button>
        <button @click="$emit('showAddService')" class="glass-button px-3 py-2 rounded-lg text-sm">
          Add Service
        </button>
      </div>
    </div>
    <div v-if="torServicesLoading && torServices.length === 0" class="text-sm text-white/40 py-4 text-center">Loading Tor services...</div>
    <div v-else-if="torServices.length === 0" class="text-sm text-white/40 py-4 text-center">No Tor services configured</div>
    <div v-else class="space-y-2">
      <div v-if="torServicesLoading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing Tor services...
      </div>
      <div v-for="svc in torServices" :key="svc.name" class="bg-black/20 rounded-xl border border-white/10 p-3">
        <div class="flex items-center justify-between gap-3">
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-white text-sm font-medium">{{ svc.name }}</p>
              <span class="text-white/30 text-xs">:{{ svc.local_port }}</span>
              <span v-if="svc.protocol" class="text-xs px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400">protocol</span>
              <span v-else-if="!svc.unauthenticated" class="text-xs px-1.5 py-0.5 rounded bg-green-500/20 text-green-400">auth</span>
              <span v-else class="text-xs px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-400">open</span>
            </div>
            <p v-if="svc.onion_address" class="text-amber-300/80 text-xs font-mono truncate cursor-pointer" :title="svc.onion_address" @click="$emit('copyAddress', svc.onion_address)">{{ svc.onion_address }}</p>
            <p v-else-if="svc.enabled" class="text-white/30 text-xs">Waiting for .onion address...</p>
            <p v-else class="text-white/30 text-xs">Disabled</p>
          </div>
          <!-- Desktop: compact inline actions next to the toggle -->
          <div class="hidden md:flex items-center gap-2 shrink-0">
            <button
              v-if="svc.onion_address && svc.enabled"
              @click="$emit('rotateService', svc.name)"
              :disabled="torRotating === svc.name"
              class="glass-button px-3 py-1.5 rounded-lg text-xs"
            >
              {{ torRotating === svc.name ? 'Rotating...' : 'Rotate' }}
            </button>
            <button
              v-if="svc.name !== 'archipelago'"
              @click="$emit('deleteService', svc.name)"
              :disabled="torDeleting === svc.name"
              class="glass-button px-2 py-1.5 rounded-lg text-xs text-red-400 hover:text-red-300"
              :title="'Delete ' + svc.name + ' hidden service'"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
            </button>
            <ToggleSwitch :model-value="svc.enabled" @update:model-value="$emit('toggleApp', svc.name, $event)" />
          </div>
          <ToggleSwitch class="shrink-0 md:hidden" :model-value="svc.enabled" @update:model-value="$emit('toggleApp', svc.name, $event)" />
        </div>
        <!-- Mobile: actions in their own 50/50 row — Delete on the LEFT, far
             from the toggle above, so a rushed thumb can't hit the wrong control. -->
        <div
          v-if="svc.name !== 'archipelago' || (svc.onion_address && svc.enabled)"
          class="grid md:hidden grid-cols-2 gap-2 mt-3"
        >
          <button
            v-if="svc.name !== 'archipelago'"
            @click="$emit('deleteService', svc.name)"
            :disabled="torDeleting === svc.name"
            class="glass-button py-2 rounded-lg text-xs text-red-400 hover:text-red-300 disabled:opacity-50"
            :class="{ 'col-span-2': !(svc.onion_address && svc.enabled) }"
            :title="'Delete ' + svc.name + ' hidden service'"
          >
            {{ torDeleting === svc.name ? 'Deleting...' : 'Delete' }}
          </button>
          <button
            v-if="svc.onion_address && svc.enabled"
            @click="$emit('rotateService', svc.name)"
            :disabled="torRotating === svc.name"
            class="glass-button py-2 rounded-lg text-xs disabled:opacity-50"
            :class="{ 'col-span-2': svc.name === 'archipelago' }"
          >
            {{ torRotating === svc.name ? 'Rotating...' : 'Rotate' }}
          </button>
        </div>
      </div>
    </div>
    <div class="responsive-card-actions-bottom-grid mt-auto pt-4 grid-cols-2 gap-3">
      <button @click="$emit('restartTor')" :disabled="torRestarting" class="mobile-card-action glass-button rounded-lg text-sm font-medium disabled:opacity-50">
        {{ torRestarting ? 'Restarting...' : 'Restart Tor' }}
      </button>
      <button @click="$emit('showAddService')" class="mobile-card-action glass-button rounded-lg text-sm font-medium">
        Add Service
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import ToggleSwitch from '@/components/ToggleSwitch.vue'

export interface TorServiceInfo {
  name: string
  local_port: number
  onion_address: string | null
  enabled: boolean
  unauthenticated: boolean
  protocol: boolean
}

defineProps<{
  torServices: TorServiceInfo[]
  torServicesLoading: boolean
  torDaemonRunning: boolean
  torRestarting: boolean
  torRotating: string | false
  torDeleting: string | false
}>()

defineEmits<{
  restartTor: []
  showAddService: []
  copyAddress: [address: string]
  rotateService: [name: string]
  deleteService: [name: string]
  toggleApp: [name: string, enabled: boolean]
}>()
</script>
