<template>
  <!-- Node Visibility -->
  <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="glass-card p-6 flex flex-col" style="--stagger-index: 3">
    <div class="mb-4 shrink-0">
      <div class="flex items-center sm:items-start gap-4">
        <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
          </svg>
        </div>
        <div class="flex-1 min-w-0">
          <h2 class="text-xl font-semibold text-white sm:mb-2">{{ t('web5.nodeVisibility') }}</h2>
          <!-- Desktop: description beside the icon, as before -->
          <p class="hidden sm:block text-white/70 text-sm">
            Make your node publicly discoverable. When enabled, anyone on the Nostr
            network can find your node and request a connection — requests always
            wait for your approval and join as a Peer, never trusted.
          </p>
        </div>
        <div v-if="visibilityLoading" class="shrink-0">
          <svg class="animate-spin h-5 w-5 text-white/40" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
        </div>
      </div>
      <!-- Mobile: description stacked under the icon + title -->
      <p class="sm:hidden mt-3 text-white/70 text-sm">
        Make your node publicly discoverable. When enabled, anyone on the Nostr
        network can find your node and request a connection — requests always
        wait for your approval and join as a Peer, never trusted.
      </p>
    </div>

    <!-- Enable switch -->
    <div class="flex items-center justify-between gap-3 p-3 rounded-lg bg-white/5 border border-white/10">
      <div class="min-w-0">
        <p class="text-sm font-medium text-white">Enable</p>
        <p class="text-xs text-white/50">
          {{ discoverEnabled ? 'Your node is public — anyone can discover it and request to peer' : 'Your node is hidden from discovery' }}
        </p>
      </div>
      <ToggleSwitch
        :model-value="discoverEnabled"
        :disabled="settingVisibility"
        aria-label="Enable public discoverability"
        @update:model-value="toggleDiscoverable"
      />
    </div>

    <!-- The node's published npub (shown when discoverable) — this, not the
         onion, is what the presence event actually posts to the relays -->
    <div v-if="discoverEnabled && nodeNpub" class="mt-4 p-3 bg-white/5 rounded-lg">
      <div class="flex items-center justify-between gap-2">
        <div class="min-w-0">
          <p class="text-xs text-white/50 mb-1">{{ t('web5.yourNodeNpub') }}</p>
          <p v-if="nodeName" class="text-sm text-white/90 truncate mb-0.5">{{ nodeName }}</p>
          <!-- Middle-ellipsis, never CSS truncate: the tail is the part a
               human compares against another listing, so it must stay visible -->
          <p class="text-xs font-mono text-white/80 truncate" :title="nodeNpub">{{ midNpub(nodeNpub) }}</p>
        </div>
        <button @click="copyNpub" class="shrink-0 p-2 rounded-lg text-white/50 hover:text-white hover:bg-white/10 transition-colors" title="Copy">
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3" />
          </svg>
        </button>
      </div>
    </div>

    <!-- Discoverable nodes -->
    <div v-if="discoverEnabled" class="mt-4 flex-1 min-h-0">
      <p class="text-sm font-medium text-white mb-2">Discoverable nodes</p>
      <div v-if="discovering && discoveredNodes.length === 0" class="py-4 text-center text-white/45 text-xs">
        Searching relays…
      </div>
      <div v-else-if="discoveredNodes.length === 0" class="py-4 text-center text-white/40 text-xs">
        No discoverable nodes found yet. Nodes appear here as relays gossip their presence.
      </div>
      <!-- min 14rem, otherwise scale with the viewport so a long discovery
           list uses the screen instead of cramming into a fixed 224px box -->
      <div v-else class="space-y-2 max-h-[max(14rem,45vh)] overflow-y-auto pr-1">
        <div
          v-for="node in discoveredNodes"
          :key="node.nostr_pubkey"
          class="p-3 bg-white/5 rounded-lg border border-white/10 flex items-start justify-between gap-3"
        >
          <div class="min-w-0 flex-1">
            <div class="text-sm text-white truncate">{{ node.name || shortNpub(node.nostr_npub) }}</div>
            <div v-if="node.name" class="text-[11px] text-white/50 font-mono truncate">{{ shortNpub(node.nostr_npub) }}</div>
            <div class="text-[11px] text-white/40 font-mono truncate">{{ node.did }}</div>
            <div class="text-[10px] text-white/30 mt-1">version {{ node.version || '?' }}</div>
          </div>
          <button
            class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50 shrink-0"
            :disabled="requestingPeer === node.nostr_pubkey || requestedPeers.has(node.nostr_pubkey)"
            @click="requestModalTarget = node"
          >
            {{ requestedPeers.has(node.nostr_pubkey) ? 'Requested' : requestingPeer === node.nostr_pubkey ? 'Sending…' : 'Request to Peer' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Refresh — full-width card action on every screen size -->
    <button
      v-if="discoverEnabled"
      class="mt-4 w-full glass-button rounded-lg py-2.5 text-sm font-medium text-white/90 hover:text-white disabled:opacity-50"
      :disabled="discovering"
      @click="discoverNodes"
    >
      {{ discovering ? 'Searching…' : 'Refresh' }}
    </button>

    <PeerRequestModal
      :show="requestModalTarget !== null"
      :target-label="requestModalTarget ? shortNpub(requestModalTarget.nostr_npub) : ''"
      :sending="requestingPeer !== null"
      @send="confirmPeerRequest"
      @cancel="requestModalTarget = null"
    />

    <!-- Name prompt on the way to discoverable: the announcement is public,
         so the name travels with it. Blank is fine — npub-only listing. -->
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="showNameModal" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click.self="cancelNameModal">
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
          <div class="glass-card p-6 max-w-md w-full relative z-10">
            <h3 class="text-lg font-semibold text-white mb-2">Name your node</h3>
            <p class="text-sm text-white/60 mb-4">Other nodes will see this name next to your npub in their discovery list. It's public. Leave blank to list as npub only.</p>
            <input
              v-model="nameInput"
              type="text"
              maxlength="32"
              placeholder="e.g. Dorian's basement node"
              class="w-full bg-black/30 border border-white/10 rounded-lg px-3 py-2 text-sm text-white placeholder-white/30 focus:outline-none focus:border-orange-500/50 mb-4"
              @keyup.enter="confirmNameModal"
            />
            <div class="flex gap-3">
              <button @click="cancelNameModal" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
              <button @click="confirmNameModal" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm font-medium bg-orange-500/20 border-orange-500/30">Turn on discovery</button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import ToggleSwitch from '@/components/ToggleSwitch.vue'
import PeerRequestModal from '@/components/federation/PeerRequestModal.vue'
import { safeClipboardWrite } from './utils'
import type { VisibilityLevel } from './types'

const { t } = useI18n()

defineProps<{
  showStagger: boolean
}>()

const emit = defineEmits<{
  toast: [text: string]
}>()

const nodeVisibility = ref<VisibilityLevel>('hidden')
const nodeNpub = ref<string | null>(null)
const nodeName = ref<string | null>(null)
const visibilityLoading = ref(false)
const settingVisibility = ref(false)
const discoverEnabled = ref(false)
// Name-prompt state: turning discovery ON routes through a small dialog so
// the operator can (optionally) name the node before it announces itself.
const showNameModal = ref(false)
const nameInput = ref('')

interface DiscoverableNode {
  nostr_pubkey: string
  nostr_npub: string
  did: string
  version: string
  name?: string | null
}

const discoveredNodes = ref<DiscoverableNode[]>([])
const discovering = ref(false)
const requestingPeer = ref<string | null>(null)
const requestedPeers = ref(new Set<string>())

/** Own-npub display: keep the start and the FULL tail visible, ellipsis in
 *  the middle. (shortNpub below stays as-is — it formats the discovered list.) */
function midNpub(npub: string): string {
  return npub.length > 24 ? `${npub.slice(0, 12)}…${npub.slice(-10)}` : npub
}

function shortNpub(npub: string): string {
  if (!npub) return 'unknown'
  return npub.length > 21 ? `${npub.slice(0, 12)}…${npub.slice(-6)}` : npub
}

async function loadVisibility() {
  visibilityLoading.value = true
  try {
    // Nostr discovery is the functional flag: when on, a presence event
    // (DID + npub — never the onion) is published to public relays so
    // ANYONE can find this node and request a connection. The legacy
    // network.get-visibility tri-state is only read for the onion display.
    const [disc, vis] = await Promise.all([
      rpcClient.nostrDiscoveryStatus(),
      rpcClient
        .call<{ visibility: string; onion_address?: string; tor_address?: string }>({ method: 'network.get-visibility' })
        .catch(() => null),
    ])
    discoverEnabled.value = !!disc.enabled
    nodeNpub.value = disc.npub || null
    nodeName.value = disc.name || null
    nodeVisibility.value = (vis?.visibility as VisibilityLevel) || 'hidden'
    if (discoverEnabled.value) void discoverNodes()
  } catch {
    discoverEnabled.value = false
  } finally {
    visibilityLoading.value = false
  }
}

async function toggleDiscoverable(enabled: boolean) {
  if (settingVisibility.value) return
  if (enabled) {
    // Turning ON goes through the name dialog: the node is about to announce
    // itself publicly, and this is the natural moment to (optionally) name it.
    nameInput.value = nodeName.value || ''
    showNameModal.value = true
    return
  }
  await applyDiscovery(false)
}

function cancelNameModal() {
  showNameModal.value = false
  // The switch never actually flipped server-side; snap the UI back.
  discoverEnabled.value = false
}

async function confirmNameModal() {
  showNameModal.value = false
  // Send exactly what's in the box: text sets the name, blank clears it.
  await applyDiscovery(true, nameInput.value.trim())
}

async function applyDiscovery(enabled: boolean, name?: string) {
  settingVisibility.value = true
  try {
    // Public means public: the switch drives nostr presence publishing.
    const res = await rpcClient.nostrSetDiscovery(enabled, name)
    discoverEnabled.value = !!res.enabled
    // Keep the legacy visibility string in sync (cosmetic; best-effort).
    const level: VisibilityLevel = enabled ? 'public' : 'hidden'
    rpcClient
      .call({ method: 'network.set-visibility', params: { visibility: level } })
      .then(() => { nodeVisibility.value = level })
      .catch(() => {})
    emit('toast', enabled ? 'Node is now publicly discoverable' : 'Node hidden from discovery')
    if (enabled) {
      if (name !== undefined) nodeName.value = name || null
      // Re-read status so the npub/name shown reflect post-enable state
      // without a page reload.
      rpcClient.nostrDiscoveryStatus()
        .then((s) => { nodeNpub.value = s.npub || null; nodeName.value = s.name || null })
        .catch(() => {})
      void discoverNodes()
    } else {
      discoveredNodes.value = []
    }
  } catch {
    emit('toast', t('web5.failedToUpdateVisibility'))
  } finally {
    settingVisibility.value = false
  }
}

async function discoverNodes() {
  if (discovering.value) return
  discovering.value = true
  try {
    const res = await rpcClient.handshakeDiscover()
    discoveredNodes.value = res.nodes || []
  } catch {
    // keep whatever we had; the empty-state copy explains relay gossip lag
  } finally {
    discovering.value = false
  }
}

const requestModalTarget = ref<DiscoverableNode | null>(null)

async function confirmPeerRequest(message: string | undefined) {
  const node = requestModalTarget.value
  if (!node) return
  await requestToPeer(node, message)
  requestModalTarget.value = null
}

async function requestToPeer(node: DiscoverableNode, message?: string) {
  if (requestingPeer.value) return
  requestingPeer.value = node.nostr_pubkey
  try {
    // Connection requests always land as Peer (observer) on approval —
    // never trusted — so a mistaken request can't hand over fleet access.
    await rpcClient.handshakeConnect(node.nostr_pubkey, message)
    requestedPeers.value.add(node.nostr_pubkey)
    requestedPeers.value = new Set(requestedPeers.value)
    emit('toast', 'Peer request sent — awaiting their approval')
  } catch (e) {
    emit('toast', e instanceof Error ? e.message : 'Failed to send peer request')
  } finally {
    requestingPeer.value = null
  }
}

function copyNpub() {
  if (!nodeNpub.value) return
  safeClipboardWrite(nodeNpub.value)
  emit('toast', t('web5.npubCopied'))
}

defineExpose({ loadVisibility })
</script>
