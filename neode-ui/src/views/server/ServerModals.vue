<template>
  <!-- Add Tor Service Modal -->
  <Teleport to="body">
    <div v-if="showAddServiceModal" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="$emit('closeAddService')" @keydown.escape="$emit('closeAddService')">
      <div class="glass-card p-6 max-w-md w-full">
        <h3 class="text-lg font-semibold text-white mb-4">Add Tor Hidden Service</h3>

        <!-- Tabs: Installed Apps | Manual -->
        <div class="flex gap-1 mb-4 border-b border-white/10">
          <button
            @click="localTab = 'apps'"
            class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
            :class="localTab === 'apps' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
          >
            Installed Apps
          </button>
          <button
            @click="localTab = 'manual'"
            class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
            :class="localTab === 'manual' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
          >
            Manual
          </button>
        </div>

        <!-- Installed Apps tab -->
        <div v-if="localTab === 'apps'">
          <p class="text-white/60 text-sm mb-3">Select an installed app to create a .onion address for it.</p>
          <div v-if="availableAppsForTor.length === 0" class="p-4 text-center text-white/40 text-sm">
            All installed apps already have Tor services.
          </div>
          <div v-else class="space-y-2 max-h-64 overflow-y-auto">
            <button
              v-for="app in availableAppsForTor"
              :key="app.id"
              @click="$emit('createServiceForApp', app.id)"
              :disabled="addingService"
              class="w-full flex items-center justify-between p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors text-left"
            >
              <div class="min-w-0">
                <p class="text-sm font-medium text-white">{{ app.title }}</p>
                <p class="text-xs text-white/40">{{ app.id }}</p>
              </div>
              <span class="text-xs text-orange-400 shrink-0">+ Add</span>
            </button>
          </div>
        </div>

        <!-- Manual tab -->
        <div v-if="localTab === 'manual'">
          <p class="text-white/60 text-sm mb-3">Create a .onion address for any local service.</p>
          <div class="space-y-4">
            <div>
              <label class="block text-sm font-medium text-white/80 mb-1">Service Name</label>
              <input
                v-model="localName"
                class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
                placeholder="my-app"
                maxlength="64"
              />
            </div>
            <div>
              <label class="block text-sm font-medium text-white/80 mb-1">Local Port</label>
              <input
                v-model.number="localPort"
                type="number"
                min="1"
                max="65535"
                class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
                placeholder="8080"
              />
            </div>
          </div>
          <div class="flex gap-3 mt-6">
            <button
              @click="$emit('createService', localName.trim(), localPort)"
              :disabled="!localName.trim() || !localPort || addingService"
              class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {{ addingService ? 'Creating...' : 'Create Service' }}
            </button>
            <button
              @click="$emit('closeAddService')"
              class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>

        <p v-if="addServiceError" class="mt-3 text-sm text-red-400">{{ addServiceError }}</p>
      </div>
    </div>
  </Teleport>

  <!-- WiFi Scan Modal -->
  <Teleport to="body">
  <div v-if="showWifiModal" class="fixed inset-0 bg-black/60 backdrop-blur-md z-50 flex items-center justify-center p-4" @click.self="$emit('closeWifi')">
    <div class="glass-card p-6 w-full max-w-md">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold text-white">WiFi Networks</h3>
        <div class="flex items-center gap-2">
          <button @click="$emit('scanWifi')" :disabled="wifiScanning" class="text-xs text-white/50 hover:text-white disabled:opacity-40 transition-colors">Refresh</button>
          <button @click="$emit('closeWifi')" class="text-white/40 hover:text-white transition-colors">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
          </button>
        </div>
      </div>
      <template v-if="wifiScanning">
        <div class="space-y-3">
          <div v-for="i in 4" :key="i" class="p-3 bg-white/5 rounded-lg animate-pulse h-12"></div>
        </div>
      </template>
      <template v-else-if="wifiNetworks.length > 0">
        <div class="space-y-2 max-h-72 overflow-y-auto">
          <button
            v-for="net in wifiNetworks"
            :key="net.ssid"
            class="w-full flex items-center justify-between p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors text-left"
            @click="$emit('selectWifi', net)"
          >
            <div>
              <p class="text-sm font-medium text-white">{{ net.ssid }}</p>
              <p class="text-xs text-white/50">{{ net.security || 'Open' }}</p>
            </div>
            <div class="flex items-center gap-2">
              <div class="flex gap-0.5">
                <div v-for="bar in 4" :key="bar" class="w-1 rounded-full" :class="bar <= Math.ceil(net.signal / 25) ? 'bg-white/80' : 'bg-white/20'" :style="{ height: (bar * 3 + 4) + 'px' }"></div>
              </div>
              <span class="text-xs text-white/50">{{ net.signal }}%</span>
            </div>
          </button>
        </div>
      </template>
      <template v-else-if="wifiScanError">
        <div class="rounded-lg border border-red-400/20 bg-red-500/10 p-4 text-sm text-red-200">
          <p>{{ wifiScanError }}</p>
          <button @click="$emit('scanWifi')" class="mt-3 text-white/80 hover:text-white underline underline-offset-4">Try again</button>
        </div>
      </template>
      <template v-else>
        <p class="text-sm text-white/50 text-center py-8">No networks found</p>
      </template>

      <!-- WiFi password prompt -->
      <div v-if="wifiConnecting" class="mt-4 pt-4 border-t border-white/10">
        <p class="text-sm text-white/80 mb-2">Connect to <span class="font-medium text-white">{{ wifiSelectedSsid }}</span></p>
        <PasswordRevealInput
          v-model="localWifiPassword"
          placeholder="WiFi password"
          :disabled="wifiSubmitting"
          class="mb-3"
          @enter="$emit('connectWifi', localWifiPassword)"
        />
        <p v-if="wifiError" class="text-sm text-red-400 mb-3">{{ wifiError }}</p>
        <div class="flex gap-2">
          <button @click="$emit('cancelWifiConnect')" :disabled="wifiSubmitting" class="flex-1 px-3 py-2 glass-button rounded-lg text-sm">Cancel</button>
          <button @click="$emit('connectWifi', localWifiPassword)" class="flex-1 px-3 py-2 glass-button rounded-lg text-sm font-medium disabled:opacity-50 disabled:cursor-not-allowed" :disabled="!localWifiPassword || wifiSubmitting">{{ wifiSubmitting ? 'Connecting...' : 'Connect' }}</button>
        </div>
      </div>
    </div>
  </div>
  </Teleport>

  <!-- DNS Configuration Modal -->
  <Teleport to="body">
  <div v-if="showDnsModal" class="fixed inset-0 bg-black/60 backdrop-blur-md z-50 flex items-center justify-center p-4" @click.self="$emit('closeDns')">
    <div class="glass-card p-6 w-full max-w-md">
      <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold text-white">DNS Configuration</h3>
        <button @click="$emit('closeDns')" class="text-white/40 hover:text-white transition-colors">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <p class="text-sm text-white/60 mb-4">Choose a DNS provider. Providers with DoH encrypt your DNS queries.</p>

      <div class="space-y-2 mb-4">
        <button
          v-for="opt in dnsProviderOptions"
          :key="opt.value"
          class="w-full flex items-center justify-between p-3 rounded-lg transition-colors text-left"
          :class="dnsSelectedProvider === opt.value ? 'bg-white/15 border border-white/20' : 'bg-white/5 border border-transparent hover:bg-white/10'"
          @click="$emit('selectDnsProvider', opt.value)"
        >
          <div>
            <p class="text-sm font-medium text-white">{{ opt.label }}</p>
            <p class="text-xs text-white/50">{{ opt.description }}</p>
          </div>
          <span v-if="opt.doh" class="text-xs px-2 py-0.5 rounded-full bg-green-400/20 text-green-400">DoH</span>
        </button>
      </div>

      <!-- Custom servers input -->
      <div v-if="dnsSelectedProvider === 'custom'" class="mb-4">
        <label class="block text-sm text-white/70 mb-1">DNS Servers (comma-separated)</label>
        <input
          v-model="localCustomServers"
          type="text"
          placeholder="1.1.1.1, 8.8.8.8"
          class="w-full px-3 py-2 bg-white/5 border border-white/10 rounded-lg text-white text-sm placeholder-white/30 focus:outline-none focus:border-white/30"
        />
      </div>

      <!-- Current servers info -->
      <div v-if="dnsServers.length > 0" class="mb-4 p-3 bg-white/5 rounded-lg">
        <p class="text-xs text-white/50 mb-1">Current resolv.conf servers</p>
        <p class="text-sm text-white/80">{{ dnsServers.join(', ') }}</p>
      </div>

      <p v-if="dnsError" class="text-sm text-red-400 mb-3">{{ dnsError }}</p>

      <div class="flex gap-2">
        <button @click="$emit('closeDns')" class="flex-1 px-3 py-2 glass-button rounded-lg text-sm">Cancel</button>
        <button
          @click="$emit('applyDns', localCustomServers)"
          class="flex-1 px-3 py-2 glass-button rounded-lg text-sm font-medium"
          :disabled="dnsApplying || (dnsSelectedProvider === 'custom' && !localCustomServers.trim())"
        >
          {{ dnsApplying ? 'Applying...' : 'Apply' }}
        </button>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import PasswordRevealInput from '@/components/PasswordRevealInput.vue'

defineProps<{
  showAddServiceModal: boolean
  showWifiModal: boolean
  showDnsModal: boolean
  availableAppsForTor: { id: string; title: string }[]
  addingService: boolean
  addServiceError: string
  wifiScanning: boolean
  wifiNetworks: { ssid: string; signal: number; security: string }[]
  wifiConnecting: boolean
  wifiSubmitting: boolean
  wifiSelectedSsid: string
  wifiError: string
  wifiScanError: string
  dnsSelectedProvider: string
  dnsServers: string[]
  dnsApplying: boolean
  dnsError: string
  dnsProviderOptions: { value: string; label: string; description: string; doh: boolean }[]
}>()

defineEmits<{
  closeAddService: []
  createServiceForApp: [appId: string]
  createService: [name: string, port: number | null]
  closeWifi: []
  selectWifi: [network: { ssid: string; signal: number; security: string }]
  connectWifi: [password: string]
  scanWifi: []
  cancelWifiConnect: []
  closeDns: []
  selectDnsProvider: [provider: string]
  applyDns: [customServers: string]
}>()

const localTab = ref<'apps' | 'manual'>('apps')
const localName = ref('')
const localPort = ref<number | null>(null)
const localWifiPassword = ref('')
const localCustomServers = ref('')
</script>
