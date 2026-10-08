<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import BackButton from '@/components/BackButton.vue'
import { rpcClient } from '@/api/rpc-client'
import VpnDeviceSettings from './VpnDeviceSettings.vue'
interface TunnelStatus { peer_management_verified?: boolean; connected?: boolean; configured?: boolean; configured_provider?: string; provider?: string; ip_address?: string | null; wg_ip?: string | null }
interface FipsStatus { installed?: boolean; service_active?: boolean; authenticated_peer_count?: number }
interface RouterStatus { configured?: boolean; router_type?: string; address?: string }
interface Forward { id: string; service_name: string; internal_port: number; external_port: number; protocol: string }
const navigation = useRouter()
const loading = ref(true)
const error = ref('')
const tunnel = ref<TunnelStatus | null>(null)
const fips = ref<FipsStatus | null>(null)
const router = ref<RouterStatus | null>(null)
const forwards = ref<Forward[] | null>(null)
let active = true
let generation = 0
const tunnelLabel = computed(() => tunnel.value?.connected === true ? 'Connected' : tunnel.value?.connected === false ? 'Not connected' : 'Could not check')
const meshLabel = computed(() => fips.value?.service_active === true ? 'Running' : fips.value?.service_active === false ? 'Stopped' : 'Could not check')
const serviceNames: Record<string, string> = { tailscale: 'Tailscale', wireguard: 'WireGuard', 'nostr-vpn': 'Nostr VPN', nostrvpn: 'Nostr VPN' }
const provider = computed(() => serviceNames[tunnel.value?.provider ?? ''] ?? 'Private connection')
async function refresh() {
  const current = ++generation
  loading.value = true; error.value = ''
  const results = await Promise.allSettled([
    rpcClient.vpnStatus(),
    rpcClient.call<FipsStatus>({ method: 'fips.status', dedup: true, maxRetries: 1 }),
    rpcClient.call<{ forwards: Forward[] }>({ method: 'router.list-forwards', dedup: true, maxRetries: 1 }),
    rpcClient.call<RouterStatus>({ method: 'router.info', dedup: true, maxRetries: 1 }),
  ])
  if (!active || current !== generation) return
  tunnel.value = results[0].status === 'fulfilled' ? results[0].value : null
  fips.value = results[1].status === 'fulfilled' ? results[1].value : null
  const rows = results[2].status === 'fulfilled' ? results[2].value?.forwards : null
  forwards.value = Array.isArray(rows) && rows.every(r => r && typeof r.id === 'string' && typeof r.service_name === 'string' && Number.isInteger(r.internal_port) && Number.isInteger(r.external_port) && typeof r.protocol === 'string') ? rows : null
  router.value = results[3].status === 'fulfilled' ? results[3].value : null
  if (results.some(r => r.status === 'rejected') || forwards.value === null) error.value = 'Some information could not be checked. Try refreshing.'
  loading.value = false
}
onMounted(refresh)
function leave() { active = false; generation++; tunnel.value = null }
onDeactivated(leave)
onUnmounted(leave)
onActivated(() => { if (!active) { active = true; void refresh() } })
</script>

<template>
  <main class="w-full min-w-0 pb-6 text-white" data-testid="firewall-settings">
    <div class="flex items-center gap-3 mb-6" data-testid="firewall-header">
      <BackButton desktop-margin="mb-0" @click="navigation.push({ name: 'server' })" />
      <h1 class="text-lg font-semibold text-white">Firewalls &amp; tunnels</h1>
    </div>
    <div class="flex flex-wrap items-start justify-between gap-4 mb-6">
      <p class="text-sm text-white/60">Choose how your devices and apps connect to this node.</p>
      <button class="glass-button text-sm" :disabled="loading" @click="refresh">{{ loading ? 'Checking…' : 'Refresh status' }}</button>
    </div>
    <p v-if="error" role="alert" class="mb-4 rounded-lg border border-orange-400/30 bg-orange-500/10 p-4 text-sm text-orange-200">{{ error }}</p>
    <div class="grid gap-4 xl:grid-cols-2 mb-6">
      <section class="glass-card p-5 min-w-0" aria-labelledby="firewall-title">
        <h2 id="firewall-title" class="text-lg font-medium text-white">Firewall</h2>
        <p class="text-sm text-white/60 mt-1">A firewall controls which connections can reach your node.</p>
        <dl class="mt-4 space-y-3 text-sm">
          <div class="status-row"><dt>Node firewall</dt><dd>Not checked</dd></div>
          <div class="status-row"><dt>Private mesh</dt><dd>{{ meshLabel }}</dd></div>
          <div class="status-row"><dt>Verified mesh connections</dt><dd>{{ fips?.authenticated_peer_count ?? 'Could not check' }}</dd></div>
        </dl>
        <p class="mt-4 text-sm text-white/50">This screen cannot yet check or change the node's firewall rules. A running mesh does not confirm firewall protection.</p>
      </section>
      <section class="glass-card p-5 min-w-0" aria-labelledby="tunnel-title">
        <h2 id="tunnel-title" class="text-lg font-medium text-white">Private connections</h2>
        <p class="text-sm text-white/60 mt-1">Tunnels let your devices reach this node through a private connection.</p>
        <dl class="mt-4 space-y-3 text-sm">
          <div class="status-row"><dt>Connection</dt><dd :class="tunnel?.connected === true ? 'text-green-300' : ''">{{ tunnelLabel }}</dd></div>
          <div class="status-row"><dt>Detected service</dt><dd>{{ tunnel ? provider : 'Could not check' }}</dd></div>
          <div class="status-row"><dt>WireGuard device address</dt><dd>{{ tunnel?.wg_ip || (tunnel ? 'Not ready' : 'Could not check') }}</dd></div>
        </dl>
        <p v-if="tunnel?.wg_ip" class="mt-4 text-sm text-white/60">Your device tunnel is already set up.</p>
      </section>
    </div>
    <VpnDeviceSettings :status-known="tunnel !== null" :configured="!!tunnel?.wg_ip" :management-verified="tunnel?.peer_management_verified === true" />
    <section class="glass-card p-5 mt-6 min-w-0" aria-labelledby="router-title">
      <h2 id="router-title" class="text-lg font-medium text-white">Your router</h2>
      <p class="text-sm text-white/60 mt-1">Manage a supported OpenWrt router, including its Wi-Fi and internet connection.</p>
      <dl class="mt-4 text-sm"><div class="status-row"><dt>Router connection saved here</dt><dd>{{ router?.configured === true ? 'Saved' : router?.configured === false ? 'Not saved here' : 'Could not check' }}</dd></div></dl>
      <p v-if="router?.configured === false" class="mt-3 text-sm text-white/50">No router connection is saved in this section. Your router and tunnels may already be configured elsewhere.</p>
      <RouterLink to="/dashboard/server/openwrt" class="glass-button mt-4 text-sm">Open router settings</RouterLink>
    </section>
    <section class="glass-card p-5 mt-6 min-w-0" aria-labelledby="public-title">
      <h2 id="public-title" class="text-lg font-medium text-white">Saved public access entries</h2>
      <p class="text-sm text-white/60 mt-1">These saved entries describe ports for apps. They do not confirm that your router has opened them.</p>
      <ul v-if="forwards?.length" class="mt-4 grid gap-3 sm:grid-cols-2">
        <li v-for="forward in forwards" :key="forward.id" class="rounded-lg bg-white/5 p-3 min-w-0"><h3 class="text-sm text-white break-words">{{ forward.service_name }}</h3><p class="mt-1 text-sm text-white/60">Outside port {{ forward.external_port }} → app port {{ forward.internal_port }} ({{ forward.protocol }})</p></li>
      </ul>
      <p v-else class="mt-4 text-sm text-white/50">{{ forwards ? 'No entries are saved here.' : 'Could not check saved entries.' }}</p>
      <p class="mt-4 text-sm text-white/50">Use your router's settings to change public access. Opening an app to the internet may also require that app's own access settings.</p>
    </section>
  </main>
</template>
<style scoped>
.status-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); align-items: start; gap: 1rem; }
.status-row dt { color: rgb(255 255 255 / 0.65); }
.status-row dd { margin: 0; text-align: right; overflow-wrap: anywhere; }
</style>
