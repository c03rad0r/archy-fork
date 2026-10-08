<template>
  <Teleport to="body" :disabled="inline">
    <Transition name="modal">
      <div
        v-if="visible"
        :class="inline ? 'mb-6' : 'fixed inset-0 z-[3000] flex items-center justify-center p-4'"
        @click.self="$emit('close')"
      >
        <div v-if="!inline" class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div :class="inline ? 'glass-card p-6' : 'glass-card p-6 max-w-2xl w-full max-h-[80vh] overflow-y-auto relative z-10'">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-xl font-semibold text-white">Discover Nodes</h2>
              <p class="text-xs text-white/60 mt-1">
                Browses Nostr presence events from configured relays. Sending a
                peer request never reveals your onion — only your DID + npub +
                an optional message travel inside an encrypted DM.
              </p>
            </div>
            <button v-if="!inline" aria-label="Close discovery" @click="$emit('close')" class="text-white/40 hover:text-white/70 transition-colors">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <div class="flex items-center gap-3 mb-4">
            <button
              class="px-4 py-2 glass-button rounded text-sm text-white/90 hover:text-white disabled:opacity-50"
              :disabled="loading"
              @click="refresh"
            >
              {{ loading ? 'Searching…' : 'Search Relays' }}
            </button>
            <span v-if="lastSearchAt" class="text-[11px] text-white/40">
              Last search: {{ lastSearchAt }}
            </span>
          </div>

          <input v-model="search" aria-label="Search discovered nodes" placeholder="Search node identity…" class="w-full mb-3 bg-black/30 text-white text-sm rounded px-3 py-2 border border-white/10" />
          <p class="mb-4 text-xs text-white/50">Presence is a discovery advertisement. It does not confirm that a node is reachable or connected. New requests grant Observer access.</p>
          <div v-if="error" class="mb-4 text-sm text-red-400">{{ error }}</div>

          <!-- Manual entry: paste an npub directly -->
          <div class="mb-6 p-3 bg-white/5 rounded-lg border border-white/10">
            <p class="text-xs text-white/60 mb-2">
              Already know an npub? Send a peer request directly.
            </p>
            <div class="flex flex-col sm:flex-row gap-2">
              <input
                v-model="manualNpub"
                placeholder="npub1…"
                class="flex-1 bg-black/30 text-white text-xs rounded px-3 py-2 border border-white/10 focus:border-orange-400/50 focus:outline-none font-mono"
              />
              <button
                class="px-4 py-2 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50"
                :disabled="!manualNpub.trim() || sendingTo === manualNpub.trim()"
                @click="sendDirect()"
              >
                Send Request
              </button>
            </div>
          </div>

          <div v-if="nodes.length === 0 && !loading" class="text-center py-8 text-white/40 text-sm">
            No discoverable nodes found. Either no peers are advertising on the
            configured relays, or your discoverability hasn't been enabled long
            enough for relays to gossip yours.
          </div>

          <div v-else class="space-y-2">
            <p v-if="nodes.length && !filteredNodes.length" class="text-sm text-white/60">No nodes match your search.</p>
            <div v-if="loading && nodes.length > 0" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
              <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Searching relays...
            </div>
            <div
              v-for="node in filteredNodes"
              :key="node.nostr_pubkey"
              class="p-3 bg-white/5 rounded-lg border border-white/10"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0 flex-1">
                  <div class="text-sm text-white truncate">
                    {{ node.name || shortNpub(node.nostr_npub) }}
                  </div>
                  <div class="text-[11px] text-white/40 font-mono truncate">{{ node.did }}</div>
                  <div class="text-[10px] text-white/30 mt-1">version {{ node.version || '?' }}</div>
                </div>
                <button
                  class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50 shrink-0"
                  :disabled="sendingTo !== null"
                  @click="actOn(node)"
                >
                  {{ statusFor(node) }}
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <PeerRequestModal
      :show="visible && requestTarget !== null"
      :target-label="requestTarget?.label ?? ''"
      :error="error"
      :sending="sendingTo !== null && sendingTo === requestTarget?.target"
      @send="confirmRequest"
      @cancel="requestTarget = null"
    />
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from 'vue'
import { rpcClient, type PendingPeerRequest } from '@/api/rpc-client'
import PeerRequestModal from '@/components/federation/PeerRequestModal.vue'

interface DiscoverableNode {
  nostr_pubkey: string
  nostr_npub: string
  did: string
  version: string
  name?: string | null
}

const props = defineProps<{
  visible: boolean
  inline?: boolean
  peerDids?: string[]
  /// Existing requests select their Requests view instead of sending again.
  outboundSent: PendingPeerRequest[]
}>()

const emit = defineEmits<{
  close: []
  viewRequest: []
  openNode: [did: string]
  /// Fired after a successful send so the parent can refresh its
  /// pending-requests list to show the new "Sent" row.
  sent: []
}>()

const nodes = ref<DiscoverableNode[]>([])
const loading = ref(false)
const error = ref('')
const lastSearchAt = ref('')
const sendingTo = ref<string | null>(null)
const manualNpub = ref('')
const search = ref('')
const filteredNodes = computed(() => {
  const term = search.value.trim().toLowerCase()
  return nodes.value.filter(node => `${node.name ?? ''} ${node.did} ${node.nostr_npub}`.toLowerCase().includes(term))
})
let discoveryGeneration = 0

watch(
  () => props.visible,
  (v) => {
    if (v && nodes.value.length === 0) void refresh()
    if (!v) { discoveryGeneration++; loading.value = false }
  },
  { immediate: true },
)
onUnmounted(() => { discoveryGeneration++ })

async function refresh() {
  if (loading.value) return
  const generation = ++discoveryGeneration
  loading.value = true
  error.value = ''
  try {
    const result = await rpcClient.handshakeDiscover()
    if (generation !== discoveryGeneration) return
    nodes.value = result.nodes
    lastSearchAt.value = new Date().toLocaleTimeString()
  } catch (e: unknown) {
    if (generation === discoveryGeneration) error.value = e instanceof Error ? e.message : 'Discovery failed'
  } finally {
    if (generation === discoveryGeneration) loading.value = false
  }
}

// Request confirmation modal: peer requests always offer an optional
// message before anything is sent (Request/Cancel).
const requestTarget = ref<{ target: string; label: string; clearManual: boolean } | null>(null)

function actOn(node: DiscoverableNode) {
  if (props.peerDids?.includes(node.did)) { emit('openNode', node.did); return }
  if (hasRequest(node.nostr_pubkey)) { emit('viewRequest'); return }
  sendTo(node)
}
function isCurrentRequest(r: PendingPeerRequest): boolean {
  if (r.state === 'pending' || r.state === 'sent') return true
  const age = Date.now() - new Date(r.received_at).getTime()
  return r.state === 'approved' && (!Number.isFinite(age) || age <= 30 * 86400000)
}
function hasRequest(key: string): boolean {
  return props.outboundSent.some(r => r.from_nostr_pubkey === key && isCurrentRequest(r))
}
function sendTo(node: DiscoverableNode) {
  error.value = ''
  requestTarget.value = { target: node.nostr_pubkey, label: shortNpub(node.nostr_npub), clearManual: false }
}

function sendDirect() {
  const v = manualNpub.value.trim().toLowerCase()
  if (!v || sendingTo.value !== null) return
  const known = nodes.value.find(node => node.nostr_npub === v || node.nostr_pubkey === v)
  if (known) { actOn(known); return }
  if (props.outboundSent.some(r => (r.from_nostr_npub === v || r.from_nostr_pubkey === v) && isCurrentRequest(r))) { emit('viewRequest'); return }
  error.value = ''
  requestTarget.value = { target: v, label: v.length > 21 ? `${v.slice(0, 12)}…${v.slice(-6)}` : v, clearManual: true }
}

async function confirmRequest(message: string | undefined) {
  const req = requestTarget.value
  if (!req || sendingTo.value !== null) return
  const sent = await sendInternal(req.target, message)
  if (!sent || requestTarget.value !== req) return
  if (req.clearManual) manualNpub.value = ''
  requestTarget.value = null
}

async function sendInternal(target: string, message?: string): Promise<boolean> {
  if (sendingTo.value !== null) return false
  sendingTo.value = target
  error.value = ''
  try {
    await rpcClient.handshakeConnect(target, message)
    emit('sent')
    return true
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : 'Send failed'
    return false
  } finally {
    sendingTo.value = null
  }
}

function statusFor(node: DiscoverableNode): string {
  if (sendingTo.value === node.nostr_pubkey) return 'Sending…'
  if (props.peerDids?.includes(node.did)) return 'Open node'
  if (hasRequest(node.nostr_pubkey)) return 'View request'
  return 'Send Request'
}

function shortNpub(npub: string): string {
  if (!npub || npub.length < 16) return npub
  return `${npub.slice(0, 14)}…${npub.slice(-8)}`
}

defineExpose({ refresh })
</script>
