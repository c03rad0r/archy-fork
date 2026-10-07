<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'

interface TunnelStatus {
  connected?: boolean
  provider?: string
  ip_address?: string
  wg_ip?: string
}

interface FipsStatus {
  installed?: boolean
  service_active?: boolean
  anchor_connected?: boolean
  authenticated_peer_count?: number
}

interface Forward { [key: string]: unknown }

const loading = ref(true)
const error = ref('')
const tunnel = ref<TunnelStatus | null>(null)
const fips = ref<FipsStatus | null>(null)
const forwards = ref<Forward[]>([])

const tunnelLabel = computed(() => {
  if (!tunnel.value) return 'Unknown'
  return tunnel.value.connected ? 'Connected' : 'Not connected'
})
const firewallLabel = computed(() => {
  if (!fips.value) return 'Unknown'
  if (fips.value.service_active === false) return 'Inactive'
  return 'Protected'
})

async function refresh() {
  loading.value = true
  error.value = ''
  const results = await Promise.allSettled([
    rpcClient.vpnStatus(),
    rpcClient.call<FipsStatus>({ method: 'fips.status', dedup: true, maxRetries: 1 }),
    rpcClient.call<{ forwards?: Forward[] }>({ method: 'router.list-forwards', dedup: true, maxRetries: 1 }),
  ])
  const failures = results.filter((result) => result.status === 'rejected')
  if (results[0]?.status === 'fulfilled') tunnel.value = results[0].value
  if (results[1]?.status === 'fulfilled') fips.value = results[1].value
  if (results[2]?.status === 'fulfilled') forwards.value = results[2].value.forwards ?? []
  if (failures.length === results.length) error.value = 'Could not load network protection status'
  else if (failures.length) error.value = 'Some checks are unavailable; verify each route before changing it'
  loading.value = false
}

onMounted(refresh)
</script>

<template>
  <main class="max-w-5xl mx-auto px-4 sm:px-6 py-6">
    <div class="flex items-start justify-between gap-4 mb-6">
      <div>
        <RouterLink to="/dashboard/server" class="text-xs text-white/50 hover:text-white">← Network</RouterLink>
        <h1 class="text-2xl font-semibold text-white mt-2">Firewall &amp; tunnels</h1>
        <p class="text-sm text-white/60 mt-1">Review reachability before exposing a service. Transport access and app authentication are separate controls.</p>
      </div>
      <button class="glass-button px-3 py-2 text-sm shrink-0" :disabled="loading" @click="refresh">
        <span v-if="loading" class="inline-flex items-center gap-2"><span class="h-3.5 w-3.5 rounded-full border-2 border-white/30 border-t-white animate-spin" /> Checking…</span>
        <span v-else>Refresh checks</span>
      </button>
    </div>

    <div v-if="error" class="mb-4 rounded-lg border border-orange-400/30 bg-orange-500/10 px-4 py-3 text-sm text-orange-200">{{ error }}</div>

    <section class="grid gap-4 md:grid-cols-2 mb-6">
      <div class="glass-card p-5">
        <h2 class="text-lg font-medium text-white">Firewall</h2>
        <p class="text-sm text-white/60 mt-1">Node protection status and mesh reachability.</p>
        <div class="mt-4 flex items-center justify-between"><span class="text-white/70">Node firewall</span><span :class="firewallLabel === 'Protected' ? 'text-green-300' : 'text-orange-300'">{{ firewallLabel }}</span></div>
        <div class="mt-2 flex items-center justify-between"><span class="text-white/70">Authenticated mesh peers</span><span class="text-white">{{ fips?.authenticated_peer_count ?? 'Unknown' }}</span></div>
      </div>
      <div class="glass-card p-5">
        <h2 class="text-lg font-medium text-white">Tunnel</h2>
        <p class="text-sm text-white/60 mt-1">Connection and handshake status.</p>
        <div class="mt-4 flex items-center justify-between"><span class="text-white/70">Tunnel</span><span :class="tunnel?.connected ? 'text-green-300' : 'text-orange-300'">{{ tunnelLabel }}</span></div>
        <div class="mt-2 flex items-center justify-between"><span class="text-white/70">Address</span><span class="text-white font-mono text-xs">{{ tunnel?.wg_ip || tunnel?.ip_address || 'Unknown' }}</span></div>
      </div>
    </section>

    <section class="glass-card p-5 mb-6">
      <div class="flex items-start justify-between gap-3"><div><h2 class="text-lg font-medium text-white">Public access checks</h2><p class="text-sm text-white/60 mt-1">Existing router forwards are shown for review. No route is changed from this screen yet.</p></div><span class="text-sm text-white/60">{{ forwards.length }} forward{{ forwards.length === 1 ? '' : 's' }}</span></div>
      <div class="mt-4 grid gap-2 sm:grid-cols-2">
        <div v-for="(forward, index) in forwards" :key="index" class="rounded-lg bg-white/5 p-3 text-xs text-white/70 font-mono break-all">{{ JSON.stringify(forward) }}</div>
        <div v-if="!forwards.length" class="text-sm text-white/45">No router forwards reported.</div>
      </div>
      <p class="text-xs text-white/45 mt-4">Public forwarding, node firewall, service listener and external reachability are independent checks. A local pass does not claim public access.</p>
    </section>

    <section class="glass-card p-5">
      <h2 class="text-lg font-medium text-white">App exposure</h2>
      <p class="text-sm text-white/60 mt-1">App manifests and owned exposure rules will be added here after validation and rollback support are qualified.</p>
      <div class="mt-4 rounded-lg border border-white/10 bg-white/5 p-3 text-sm text-white/60">No changes are available yet. Existing app access remains untouched.</div>
    </section>
  </main>
</template>
