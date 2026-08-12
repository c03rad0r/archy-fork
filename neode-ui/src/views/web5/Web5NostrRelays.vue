<template>
  <!-- Nostr Relays -->
  <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="glass-card p-6 flex flex-col" style="--stagger-index: 2">
    <div class="flex items-start gap-4 mb-4 shrink-0">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
        </svg>
      </div>
      <div class="flex-1">
        <h2 class="text-xl font-semibold text-white mb-2">{{ t('web5.nostrRelays') }}</h2>
        <p class="text-white/70 text-sm mb-4">{{ t('web5.nostrRelaysDesc') }}</p>
      </div>
    </div>

    <div class="space-y-3 flex-1 min-h-0">
      <div v-if="relaysLoading && hasRelayData" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        Refreshing relays...
      </div>
      <div v-else-if="relaysLoadError && hasRelayData" class="p-2 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-xs">
        {{ relaysLoadError }}
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.relaysConnectedLabel') }}</span>
        </div>
        <span class="text-white/60 text-sm">{{ nostrRelayStats?.connected_count ?? 0 }} active</span>
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.totalRelays') }}</span>
        </div>
        <span :class="(nostrRelayStats?.total_relays ?? 0) > 0 ? 'text-green-400' : 'text-white/60'" class="text-sm font-medium">
          {{ nostrRelayStats?.total_relays ?? 0 }} configured
        </span>
      </div>

      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('common.enabled') }}</span>
        </div>
        <span class="text-white/60 text-sm">{{ nostrRelayStats?.enabled_count ?? 0 }} relays</span>
      </div>
    </div>

    <button @click="showRelaysModal = true" class="mt-6 w-full px-4 py-2 glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors shrink-0">
      {{ t('web5.relays') }}
    </button>
  </div>

  <!-- Relay Management Modal -->
  <Teleport to="body">
  <div v-if="showRelaysModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="showRelaysModal = false" @keydown.escape="showRelaysModal = false">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="relays-title">
      <div class="flex items-center justify-between mb-4">
        <h2 id="relays-title" class="text-lg font-bold text-white">{{ t('web5.nostrRelays') }}</h2>
        <button @click="showRelaysModal = false" class="text-white/40 hover:text-white/80 transition-colors">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
        </button>
      </div>

      <!-- Relay List -->
      <div v-if="nostrRelays.length" class="space-y-2 mb-4">
        <div v-for="relay in nostrRelays" :key="relay.url" class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
          <div class="flex items-center gap-3 min-w-0 flex-1">
            <div class="w-2 h-2 rounded-full flex-shrink-0" :class="relay.connected ? 'bg-green-400' : 'bg-white/30'"></div>
            <span class="text-sm text-white font-mono truncate">{{ relay.url }}</span>
          </div>
          <div class="flex items-center gap-2 flex-shrink-0">
            <button @click="toggleNostrRelay(relay.url, !relay.enabled)" class="text-xs px-2 py-1 rounded" :class="relay.enabled ? 'bg-green-500/20 text-green-400' : 'bg-white/5 text-white/40'">
              {{ relay.enabled ? 'On' : 'Off' }}
            </button>
            <button @click="removeNostrRelay(relay.url)" class="text-white/30 hover:text-red-400 transition-colors p-1">
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
            </button>
          </div>
        </div>
      </div>
      <div v-else class="text-center text-white/40 text-sm py-4 mb-4">{{ t('web5.noRelays') }}</div>

      <!-- Add Relay -->
      <div class="border-t border-white/10 pt-4">
        <h3 class="text-sm font-semibold text-white mb-3">{{ t('web5.addRelay') }}</h3>
        <div class="flex gap-2">
          <input v-model="newRelayUrl" type="text" :placeholder="t('web5.relayUrlPlaceholder')" class="flex-1 input-glass" @keyup.enter="addNostrRelay" />
          <button @click="addNostrRelay" :disabled="!newRelayUrl.trim()" class="glass-button px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
            Add
          </button>
        </div>
        <div v-if="relayError" class="text-xs text-red-400 mt-2">{{ relayError }}</div>
      </div>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import type { NostrRelayData, NostrRelayStatsData } from './types'

const { t } = useI18n()

defineProps<{
  showStagger: boolean
}>()

const nostrRelays = ref<NostrRelayData[]>([])
const nostrRelayStats = ref<NostrRelayStatsData | null>(null)
const showRelaysModal = ref(false)
const newRelayUrl = ref('')
const relayError = ref('')
const relaysLoading = ref(false)
const relaysLoadError = ref('')
const hasRelayData = computed(() => nostrRelays.value.length > 0 || nostrRelayStats.value !== null)

async function loadNostrRelays() {
  const hadRelayData = hasRelayData.value
  relaysLoading.value = true
  relaysLoadError.value = ''
  try {
    const [relayRes, statsRes] = await Promise.all([
      rpcClient.call<{ relays: NostrRelayData[] }>({ method: 'nostr.list-relays' }),
      rpcClient.call<NostrRelayStatsData>({ method: 'nostr.get-stats' }),
    ])
    nostrRelays.value = relayRes.relays || []
    nostrRelayStats.value = statsRes
  } catch (e: unknown) {
    relaysLoadError.value = e instanceof Error ? e.message : 'Failed to load relays'
    if (!hadRelayData) {
      nostrRelays.value = []
      nostrRelayStats.value = null
    }
  } finally {
    relaysLoading.value = false
  }
}

async function addNostrRelay() {
  if (!newRelayUrl.value.trim()) return
  relayError.value = ''
  try {
    await rpcClient.call({ method: 'nostr.add-relay', params: { url: newRelayUrl.value.trim() } })
    newRelayUrl.value = ''
    await loadNostrRelays()
  } catch (e: unknown) {
    relayError.value = e instanceof Error ? e.message : t('web5.failedToAddRelay')
  }
}

async function removeNostrRelay(url: string) {
  try {
    await rpcClient.call({ method: 'nostr.remove-relay', params: { url } })
    await loadNostrRelays()
  } catch (e: unknown) {
    relayError.value = e instanceof Error ? e.message : t('web5.failedToRemoveRelay')
  }
}

async function toggleNostrRelay(url: string, enabled: boolean) {
  try {
    await rpcClient.call({ method: 'nostr.toggle-relay', params: { url, enabled } })
    await loadNostrRelays()
  } catch (e: unknown) {
    relayError.value = e instanceof Error ? e.message : t('web5.failedToToggleRelay')
  }
}

function openRelaysModal() {
  showRelaysModal.value = true
}

defineExpose({ loadNostrRelays, nostrRelayStats, openRelaysModal })
</script>
