<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="visible"
        class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
        @click.self="$emit('close')"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
        <div class="glass-card p-6 max-w-2xl w-full max-h-[80vh] overflow-y-auto relative z-10">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h2 class="text-xl font-semibold text-white">Discover Nodes</h2>
              <p class="text-xs text-white/60 mt-1">
                Browses Nostr presence events from configured relays. Sending a
                peer request never reveals your onion — only your DID + npub +
                an optional message travel inside an encrypted DM.
              </p>
            </div>
            <button @click="$emit('close')" class="text-white/40 hover:text-white/70 transition-colors">
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
            <div v-if="loading && nodes.length > 0" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
              <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Searching relays...
            </div>
            <div
              v-for="node in nodes"
              :key="node.nostr_pubkey"
              class="p-3 bg-white/5 rounded-lg border border-white/10"
            >
              <div class="flex items-start justify-between gap-3">
                <div class="min-w-0 flex-1">
                  <div class="text-sm text-white truncate">
                    {{ shortNpub(node.nostr_npub) }}
                  </div>
                  <div class="text-[11px] text-white/40 font-mono truncate">{{ node.did }}</div>
                  <div class="text-[10px] text-white/30 mt-1">version {{ node.version || '?' }}</div>
                </div>
                <button
                  class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50 shrink-0"
                  :disabled="sendingTo === node.nostr_pubkey || alreadySentTo(node.nostr_pubkey)"
                  @click="sendTo(node)"
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
      :show="requestTarget !== null"
      :target-label="requestTarget?.label ?? ''"
      :sending="sendingTo !== null && sendingTo === requestTarget?.target"
      @send="confirmRequest"
      @cancel="requestTarget = null"
    />
  </Teleport>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { rpcClient, type PendingPeerRequest } from '@/api/rpc-client'
import PeerRequestModal from '@/components/federation/PeerRequestModal.vue'

interface DiscoverableNode {
  nostr_pubkey: string
  nostr_npub: string
  did: string
  version: string
}

const props = defineProps<{
  visible: boolean
  /// Outbound rows from the parent's pending list, used to grey out
  /// "Send Request" buttons for npubs we've already requested.
  outboundSent: PendingPeerRequest[]
}>()

const emit = defineEmits<{
  close: []
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

watch(
  () => props.visible,
  (v) => {
    if (v && nodes.value.length === 0) refresh()
  },
)

async function refresh() {
  loading.value = true
  error.value = ''
  try {
    const result = await rpcClient.handshakeDiscover()
    nodes.value = result.nodes
    lastSearchAt.value = new Date().toLocaleTimeString()
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : 'Discovery failed'
  } finally {
    loading.value = false
  }
}

// Request confirmation modal: peer requests always offer an optional
// message before anything is sent (Request/Cancel).
const requestTarget = ref<{ target: string; label: string; clearManual: boolean } | null>(null)

function sendTo(node: DiscoverableNode) {
  requestTarget.value = { target: node.nostr_pubkey, label: shortNpub(node.nostr_npub), clearManual: false }
}

function sendDirect() {
  const v = manualNpub.value.trim()
  if (!v) return
  requestTarget.value = { target: v, label: v.length > 21 ? `${v.slice(0, 12)}…${v.slice(-6)}` : v, clearManual: true }
}

async function confirmRequest(message: string | undefined) {
  const req = requestTarget.value
  if (!req) return
  await sendInternal(req.target, message)
  if (req.clearManual) manualNpub.value = ''
  requestTarget.value = null
}

async function sendInternal(target: string, message?: string) {
  sendingTo.value = target
  error.value = ''
  try {
    await rpcClient.handshakeConnect(target, message)
    emit('sent')
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : 'Send failed'
  } finally {
    sendingTo.value = null
  }
}

function alreadySentTo(npubHex: string): boolean {
  return props.outboundSent.some(
    (r) => r.outbound && r.from_nostr_pubkey === npubHex && r.state === 'sent',
  )
}

function statusFor(node: DiscoverableNode): string {
  if (sendingTo.value === node.nostr_pubkey) return 'Sending…'
  if (alreadySentTo(node.nostr_pubkey)) return 'Already sent'
  return 'Send Request'
}

function shortNpub(npub: string): string {
  if (!npub || npub.length < 16) return npub
  return `${npub.slice(0, 14)}…${npub.slice(-8)}`
}

defineExpose({ refresh })
</script>
