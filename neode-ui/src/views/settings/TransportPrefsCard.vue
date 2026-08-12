<template>
  <div class="glass-card p-6 transition-all">
    <div class="flex items-center gap-3 mb-2">
      <svg class="w-6 h-6 text-cyan-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
      </svg>
      <h2 class="text-xl font-semibold text-white">Transport Preferences</h2>
    </div>
    <p class="text-white/60 text-sm mb-5">
      Pick how each service reaches federated peers. FIPS is the overlay mesh
      (fast, authenticated). Tor is the anonymous hidden-service fallback.
      <span class="text-white/40">Auto = FIPS first, Tor on failure.</span>
    </p>

    <div v-if="loading && !prefs" class="text-white/50 text-sm">Loading…</div>
    <div v-else-if="error && !prefs" class="text-red-400 text-sm">{{ error }}</div>
    <div v-else class="space-y-3">
      <div v-if="loading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing transport preferences...
      </div>
      <div v-else-if="error" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ error }}
      </div>
      <div
        v-for="svc in services"
        :key="svc.key"
        class="flex items-center justify-between gap-4 p-3 bg-white/5 rounded-lg"
      >
        <div class="min-w-0">
          <div class="text-sm font-medium text-white">{{ svc.label }}</div>
          <div class="text-xs text-white/50 truncate">{{ svc.hint }}</div>
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <button
            v-for="opt in options"
            :key="opt.value"
            type="button"
            class="px-3 py-1.5 rounded text-xs font-medium transition-colors"
            :class="classesFor(svc.key, opt.value)"
            :disabled="saving === svc.key"
            @click="setPref(svc.key, opt.value)"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'

type Pref = 'auto' | 'fips' | 'tor'
type Service = 'federation' | 'peers' | 'peer_files' | 'messaging' | 'mesh_file_sharing'

interface Preferences {
  federation: Pref
  peers: Pref
  peer_files: Pref
  messaging: Pref
  mesh_file_sharing: Pref
}

const services: { key: Service; label: string; hint: string }[] = [
  { key: 'federation', label: 'Federation', hint: 'State sync, invites, peer notifications' },
  { key: 'peers', label: 'Peer Linking', hint: 'Address + DID rotation broadcasts' },
  { key: 'peer_files', label: 'Peer Files', hint: 'Content catalog download / browse' },
  { key: 'messaging', label: 'Archipelago Messaging', hint: 'Node-to-node chat + mesh bridge' },
  { key: 'mesh_file_sharing', label: 'Mesh File Sharing', hint: 'Blob fetches for shared mesh content' },
]

const options: { value: Pref; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'fips', label: 'FIPS' },
  { value: 'tor', label: 'Tor' },
]

const prefs = ref<Preferences | null>(null)
const loading = ref(true)
const saving = ref<Service | null>(null)
const error = ref<string | null>(null)

async function load() {
  const hadPrefs = prefs.value !== null
  loading.value = true
  error.value = null
  try {
    prefs.value = await rpcClient.call<Preferences>({ method: 'transport.preferences' })
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load transport preferences'
    if (!hadPrefs) prefs.value = null
  } finally {
    loading.value = false
  }
}

function classesFor(service: Service, value: Pref): string {
  const active = prefs.value?.[service] === value
  if (active) {
    if (value === 'fips') return 'bg-cyan-500/30 text-cyan-200 ring-1 ring-cyan-400/50'
    if (value === 'tor') return 'bg-purple-500/30 text-purple-200 ring-1 ring-purple-400/50'
    return 'bg-white/20 text-white ring-1 ring-white/30'
  }
  return 'bg-white/5 text-white/60 hover:bg-white/10 hover:text-white/80'
}

async function setPref(service: Service, pref: Pref) {
  if (!prefs.value) return
  if (prefs.value[service] === pref) return
  saving.value = service
  const previous = prefs.value[service]
  prefs.value[service] = pref
  try {
    prefs.value = await rpcClient.call<Preferences>({
      method: 'transport.set-preference',
      params: { service, pref },
    })
  } catch (e) {
    if (prefs.value) prefs.value[service] = previous
    error.value = e instanceof Error ? e.message : 'Failed to save preference'
  } finally {
    saving.value = null
  }
}

onMounted(load)

defineExpose({ load })
</script>
