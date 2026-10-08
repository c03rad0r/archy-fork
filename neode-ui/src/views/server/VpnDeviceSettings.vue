<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref } from 'vue'
import DOMPurify from 'dompurify'
import { rpcClient } from '@/api/rpc-client'

const props = defineProps<{ configured: boolean; managementVerified: boolean; statusKnown: boolean }>()
interface Device { name: string; ip: string; management_state?: 'active' | 'pending' | 'revoking' }
interface Connection { qr_svg: string; config: string; peer_ip: string }
const devices = ref<Device[] | null>(null)
const loading = ref(false)
const busy = ref(false)
const error = ref('')
const notice = ref('')
const name = ref('')
const selected = ref('')
const connection = ref<Connection | null>(null)
const removeName = ref('')
const copied = ref(false)
const needsRefresh = ref(false)
const safeName = (value: string) => /^[A-Za-z0-9][A-Za-z0-9 _-]{0,63}$/.test(value)
const filename = (value: string) => value.toLowerCase().replace(/ /g, '-')
const canCreate = computed(() => props.statusKnown && props.configured && props.managementVerified && devices.value !== null && !needsRefresh.value && safeName(name.value.trim()) && !devices.value.some(d => filename(d.name) === filename(name.value.trim())))
let active = true
let request = 0
let viewGeneration = 0
const qr = computed(() => DOMPurify.sanitize(connection.value?.qr_svg ?? '', { USE_PROFILES: { svg: true } }))

async function refresh() {
  const generation = ++request
  loading.value = true
  error.value = ''
  try {
    const result = await rpcClient.call<{ peers: Device[] }>({ method: 'vpn.list-peers', dedup: true, maxRetries: 1 })
    if (!active || generation !== request) return
    if (!Array.isArray(result?.peers) || result.peers.some(p => !p || typeof p.name !== 'string' || typeof p.ip !== 'string' || (p.management_state !== undefined && !['active', 'pending', 'revoking'].includes(p.management_state)))) throw new Error('Invalid device list')
    devices.value = result.peers.map(p => ({ name: p.name, ip: p.ip, management_state: p.management_state }))
    needsRefresh.value = false
  } catch {
    if (active && generation === request) error.value = devices.value ? 'Could not refresh devices. The last list is shown.' : 'Could not load devices. Try again.'
  } finally { if (active && generation === request) loading.value = false }
}
async function openConnection(deviceName?: string) {
  if (busy.value || devices.value === null || (!deviceName && !canCreate.value)) return
  const view = viewGeneration
  const trimmed = deviceName ?? name.value.trim()
  if (!safeName(trimmed)) return
  busy.value = true; error.value = ''; notice.value = ''; connection.value = null; copied.value = false
  try {
    const result = await rpcClient.call<Connection>({ method: deviceName ? 'vpn.peer-config' : 'vpn.create-peer', params: { name: trimmed }, maxRetries: 0 })
    if (!active || view !== viewGeneration) return
    if (!result || typeof result.qr_svg !== 'string' || typeof result.config !== 'string' || typeof result.peer_ip !== 'string') throw new Error('Invalid connection details')
    selected.value = trimmed; connection.value = result; name.value = ''
    if (!deviceName) await refresh()
  } catch { if (active && view === viewGeneration) { needsRefresh.value = !deviceName; error.value = deviceName ? 'Could not load connection details. Try again.' : 'Could not confirm the new device. Refresh the list before trying again.' } }
  finally { busy.value = false }
}
async function removeDevice() {
  if (busy.value || !props.managementVerified || needsRefresh.value || !safeName(removeName.value)) return
  const view = viewGeneration
  const target = removeName.value
  busy.value = true; error.value = ''; notice.value = ''
  try {
    const result = await rpcClient.call<{ removed: boolean }>({ method: 'vpn.remove-peer', params: { name: target }, maxRetries: 0 })
    if (result?.removed !== true) throw new Error('Removal unconfirmed')
    if (!active || view !== viewGeneration) return
    if (selected.value === target) connection.value = null
    devices.value = devices.value?.filter(d => d.name !== target) ?? null
    removeName.value = ''; notice.value = `${target} was removed.`
    await refresh()
  } catch { if (active && view === viewGeneration) { needsRefresh.value = true; error.value = 'Could not confirm removal. Refresh the list to check before trying again.' } }
  finally { busy.value = false }
}
async function copyConnection() {
  if (!connection.value) return
  const view = viewGeneration
  try { await navigator.clipboard.writeText(connection.value.config); if (active && view === viewGeneration) copied.value = true }
  catch { if (active && view === viewGeneration) error.value = 'Could not copy. Use the QR code instead.' }
}
onMounted(refresh)
function leave() { active = false; request++; viewGeneration++; connection.value = null; selected.value = ''; name.value = ''; removeName.value = ''; copied.value = false }
onDeactivated(leave)
onUnmounted(leave)
onActivated(() => { if (!active) { active = true; void refresh() } })
</script>

<template>
  <section class="glass-card p-5 min-w-0" aria-labelledby="vpn-devices-title">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div><h2 id="vpn-devices-title" class="text-lg font-medium text-white">Your devices</h2><p class="mt-1 text-sm text-white/60">Give a phone or computer a private way to reach this node using the WireGuard app.</p></div>
      <button class="glass-button text-sm" :disabled="loading || busy" @click="refresh">{{ loading ? 'Checking…' : 'Refresh devices' }}</button>
    </div>
    <p v-if="statusKnown && !configured" class="mt-4 text-sm text-white/60">The device tunnel is not ready. Check its status above before adding a device.</p>
    <p v-if="statusKnown && !managementVerified" class="mt-4 text-sm text-white/60">Device changes are unavailable on this node right now. You can still view saved connection details.</p>
    <p v-if="error" role="alert" class="mt-4 text-sm text-orange-200">{{ error }}</p>
    <p v-if="notice" role="status" class="mt-4 text-sm text-green-300">{{ notice }}</p>
    <ul v-if="devices?.length" class="mt-4 space-y-2">
      <li v-for="device in devices" :key="device.name" class="flex flex-wrap items-center justify-between gap-3 rounded-lg bg-white/5 p-3">
        <div class="min-w-0"><p class="text-sm text-white break-words">{{ device.name }}</p><p class="text-xs text-white/50 break-all">{{ device.ip }}</p><p v-if="device.management_state === 'pending'" class="mt-1 text-xs text-orange-200">Setup was not confirmed. Remove this entry before adding the device again.</p><p v-if="device.management_state === 'revoking'" class="mt-1 text-xs text-orange-200">Removal needs another check. Refresh, then retry removal.</p></div>
        <div class="flex flex-wrap gap-2"><button class="glass-button text-xs" :disabled="busy || !safeName(device.name) || (device.management_state !== undefined && device.management_state !== 'active')" @click="openConnection(device.name)">Connection details</button><button class="glass-button text-xs" :disabled="busy || !managementVerified || !safeName(device.name)" :aria-label="`Remove ${device.name}`" @click="removeName = device.name">Remove</button></div>
      </li>
    </ul>
    <p v-else-if="devices" class="mt-4 text-sm text-white/50">No devices have been added.</p>
    <div v-if="removeName" class="mt-4 rounded-lg border border-orange-400/30 p-4">
      <p class="text-sm text-white">Remove {{ removeName }}? It will lose this private connection.</p>
      <div class="mt-3 flex flex-wrap gap-2"><button class="glass-button text-sm" :disabled="busy || needsRefresh" @click="removeDevice">{{ busy ? 'Removing…' : 'Remove device' }}</button><button class="glass-button text-sm" :disabled="busy" @click="removeName = ''">Keep device</button></div>
    </div>
    <form class="mt-5 flex flex-wrap items-end gap-3" @submit.prevent="openConnection()">
      <label class="min-w-0 flex-1 text-sm text-white/70" for="vpn-device-name">Device name<input id="vpn-device-name" v-model="name" :disabled="busy || !configured || !managementVerified || devices === null" maxlength="64" placeholder="For example, my phone" pattern="[A-Za-z0-9][A-Za-z0-9 _\-]{0,63}" title="Use letters, numbers, spaces, dashes or underscores." class="mt-2 block w-full rounded-lg border border-white/15 bg-white/5 px-3 py-2.5 text-white" /></label>
      <button type="submit" class="glass-button text-sm" :disabled="busy || !canCreate">{{ busy ? 'Please wait…' : 'Add device' }}</button>
    </form>
    <div v-if="connection" class="mt-5 rounded-lg border border-white/15 p-4">
      <div class="flex flex-wrap items-center justify-between gap-3"><h3 class="text-base text-white break-words">Connect {{ selected }}</h3><button class="glass-button text-sm" @click="connection = null">Hide details</button></div>
      <p class="mt-2 text-sm text-white/60">Open WireGuard on that device and scan this code. Keep these connection details private.</p>
      <div class="connection-qr mt-4 max-w-full w-fit rounded-lg bg-white p-3" v-html="qr" />
      <button class="glass-button mt-4 text-sm" @click="copyConnection">{{ copied ? 'Copied' : 'Copy connection settings' }}</button>
    </div>
  </section>
</template>
<style scoped>
.connection-qr :deep(svg) { max-width: 100%; height: auto; }
</style>
