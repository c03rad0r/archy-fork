<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { rpcClient } from '@/api/rpc-client'

interface VpnStatus {
  connected: boolean
  provider: string | null
  interface: string | null
  ip_address: string | null
  hostname: string | null
  peers_connected: number
  bytes_in: number
  bytes_out: number
}

const vpnStatus = ref<VpnStatus | null>(null)
const loading = ref(true)
const error = ref('')

async function fetchVpnStatus() {
  try {
    loading.value = true
    error.value = ''
    const result = await rpcClient.call({ method: 'vpn.status', params: {} })
    vpnStatus.value = result as VpnStatus
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to get VPN status'
  } finally {
    loading.value = false
  }
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

onMounted(fetchVpnStatus)

defineExpose({ fetchVpnStatus })
</script>

<template>
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex items-center justify-between mb-4">
      <div class="flex items-center gap-3">
        <div class="w-10 h-10 rounded-xl bg-purple-500/20 flex items-center justify-center">
          <svg class="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
          </svg>
        </div>
        <div>
          <h3 class="text-base font-semibold text-white/96">Nostr VPN</h3>
          <p class="text-sm text-white/50">Mesh VPN with Nostr signaling</p>
        </div>
      </div>
      <div v-if="vpnStatus" class="flex items-center gap-2">
        <span
          class="w-2.5 h-2.5 rounded-full"
          :class="vpnStatus.connected ? 'bg-green-400 animate-pulse' : 'bg-white/30'"
        />
        <span class="text-sm" :class="vpnStatus.connected ? 'text-green-400' : 'text-white/50'">
          {{ vpnStatus.connected ? 'Connected' : 'Inactive' }}
        </span>
      </div>
    </div>

    <div v-if="loading && !vpnStatus" class="text-sm text-white/50">Loading VPN status...</div>

    <div v-else-if="vpnStatus?.connected" class="space-y-3">
      <div v-if="loading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing VPN status...
      </div>
      <div class="grid grid-cols-2 gap-3">
        <div class="bg-white/5 rounded-lg px-3 py-2">
          <div class="text-xs text-white/50 mb-1">Provider</div>
          <div class="text-sm font-medium text-white/90">{{ vpnStatus.provider || 'wireguard' }}</div>
        </div>
        <div class="bg-white/5 rounded-lg px-3 py-2">
          <div class="text-xs text-white/50 mb-1">Peers</div>
          <div class="text-sm font-medium text-white/90">{{ vpnStatus.peers_connected }}</div>
        </div>
        <div v-if="vpnStatus.ip_address" class="bg-white/5 rounded-lg px-3 py-2">
          <div class="text-xs text-white/50 mb-1">VPN Address</div>
          <div class="text-sm font-mono text-white/90">{{ vpnStatus.ip_address }}</div>
        </div>
        <div v-if="vpnStatus.bytes_in || vpnStatus.bytes_out" class="bg-white/5 rounded-lg px-3 py-2">
          <div class="text-xs text-white/50 mb-1">Traffic</div>
          <div class="text-sm text-white/90">{{ formatBytes(vpnStatus.bytes_in) }} / {{ formatBytes(vpnStatus.bytes_out) }}</div>
        </div>
      </div>
    </div>

    <div v-else-if="vpnStatus && !vpnStatus.connected" class="space-y-2">
      <div v-if="loading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing VPN status...
      </div>
      <div class="text-sm text-white/50">
        VPN will activate automatically when peers are discovered via Nostr relays.
      </div>
    </div>

    <div v-if="error" class="text-sm text-red-400 mt-2">{{ error }}</div>
  </div>
</template>
