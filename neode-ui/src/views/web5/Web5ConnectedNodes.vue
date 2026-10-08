<template>
  <!-- Connected Nodes (P2P over Tor) -->
  <div data-controller-container tabindex="0" class="glass-card p-6 scroll-mt-24 flex flex-col min-h-0 xl:min-h-[40rem]">
    <!-- Desktop: side-by-side layout -->
    <div class="hidden md:flex items-start gap-4 mb-4">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
        </svg>
      </div>
      <div class="flex-1">
        <h2 class="text-xl font-semibold text-white mb-2">{{ t('web5.connectedNodes') }}</h2>
      </div>
    </div>
    <!-- Mobile: stacked layout -->
    <div class="md:hidden mb-4">
      <div class="flex items-center gap-4 mb-2">
        <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
          <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
        </div>
        <h2 class="text-xl font-semibold text-white">{{ t('web5.connectedNodes') }}</h2>
      </div>
    </div>

    <!-- Tabs: Trusted | Observers | Messages | Requests -->
    <div class="flex gap-1 mb-4 border-b border-white/10 overflow-x-auto whitespace-nowrap shrink-0">
      <button
        @click="nodesContainerTab = 'trusted'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="nodesContainerTab === 'trusted' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        {{ t('web5.trusted') }}
        <span v-if="peers.length > 0" class="ml-1.5 text-xs text-white/50">({{ peers.length }})</span>
      </button>
      <button
        @click="nodesContainerTab = 'observers'"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors"
        :class="nodesContainerTab === 'observers' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        {{ t('web5.observers') }}
        <span v-if="observers.length > 0" class="ml-1.5 text-xs text-white/50">({{ observers.length }})</span>
      </button>
      <button
        @click="switchToRequestsTab"
        class="px-4 py-2 text-sm font-medium rounded-t-lg transition-colors flex items-center gap-1.5"
        :class="nodesContainerTab === 'requests' ? 'bg-white/10 text-white' : 'text-white/60 hover:text-white/80 hover:bg-white/5'"
      >
        {{ t('web5.requests') }}
        <span v-if="requestCount > 0" class="ml-1.5 text-xs text-orange-400">({{ requestCount }})</span>
        <span v-if="requestCount > 0" class="w-2 h-2 rounded-full bg-orange-500 animate-pulse"></span>
      </button>
    </div>

    <p v-if="peersError && nodesContainerTab !== 'requests'" role="status" class="text-xs text-amber-200 mb-2">{{ peersError }}</p>
    <!-- Trusted tab -->
    <div v-show="nodesContainerTab === 'trusted'" class="space-y-2 min-h-0 overflow-y-auto max-h-72 xl:flex-1 xl:basis-0 xl:max-h-none">
      <div v-if="loadingPeers && peers.length === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('common.loading') }}
      </div>
      <div v-else-if="peers.length === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('web5.noPeers') }}
      </div>
      <div v-else-if="loadingPeers" class="p-2 text-center text-white/45 text-xs">
        {{ t('common.loading') }}
      </div>
      <div
        v-for="p in sortedPeers"
        :key="p.pubkey"
        @click="router.push({ path: '/dashboard/server/federation', query: { node: p.did || p.pubkey || p.onion } })"
        class="flex items-center justify-between p-3 bg-white/5 rounded-lg cursor-pointer hover:bg-white/10 transition-colors"
      >
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-2 h-2 rounded-full shrink-0" :class="statusClass(p)"></div>
          <div class="min-w-0">
            <p class="text-sm font-mono text-white/90 truncate">{{ p.name || p.onion || (p.pubkey || '').slice(0, 16) + '...' }}</p>
            <p class="text-xs text-white/50 truncate">{{ availabilityText(p) }} · {{ contactText(p) }}</p>
          </div>
        </div>
        <button
          @click.stop="router.push({ path: '/dashboard/mesh', query: { peer: p.pubkey || p.did || p.onion } })"
          class="px-2 py-1 text-xs rounded bg-orange-500/20 text-orange-400 hover:bg-orange-500/30 transition-colors shrink-0"
        >
          {{ t('web5.message') }}
        </button>
      </div>
    </div>

    <!-- Observers tab -->
    <div v-show="nodesContainerTab === 'observers'" class="space-y-2 min-h-0 overflow-y-auto max-h-72 xl:flex-1 xl:basis-0 xl:max-h-none">
      <div v-if="loadingPeers && observers.length === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('common.loading') }}
      </div>
      <div v-else-if="observers.length === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('web5.noObservers') }}
      </div>
      <div v-else-if="loadingPeers" class="p-2 text-center text-white/45 text-xs">
        {{ t('common.loading') }}
      </div>
      <div
        v-for="p in sortedObservers"
        :key="p.pubkey"
        @click="router.push({ path: '/dashboard/server/federation', query: { node: p.did || p.pubkey || p.onion } })"
        class="flex items-center justify-between p-3 bg-white/5 rounded-lg cursor-pointer hover:bg-white/10 transition-colors"
      >
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-2 h-2 rounded-full shrink-0" :class="statusClass(p)"></div>
          <div class="min-w-0">
            <p class="text-sm font-mono text-white/90 truncate">{{ p.name || p.onion || (p.pubkey || '').slice(0, 16) + '...' }}</p>
            <p class="text-xs text-white/50 truncate">{{ availabilityText(p) }} · {{ contactText(p) }}</p>
          </div>
        </div>
        <span class="px-2 py-1 text-xs rounded bg-blue-500/20 text-blue-300 shrink-0">
          {{ t('web5.observer') }}
        </span>
      </div>
    </div>

    <!-- Requests tab -->
    <div v-show="nodesContainerTab === 'requests'" class="space-y-2 min-h-0 overflow-y-auto max-h-72 xl:flex-1 xl:basis-0 xl:max-h-none">
      <p v-if="requestError" role="status" class="text-xs text-amber-200">{{ requestError }}</p>
      <button v-for="request in nostrRequests" :key="'nostr:' + request.id"
        class="w-full text-left p-3 bg-white/5 rounded-lg hover:bg-white/10 transition-colors"
        @click="router.push({ path: '/dashboard/server/federation', hash: '#connection-requests' })">
        <span class="block text-sm text-white truncate">{{ request.from_name || request.from_did || request.from_nostr_npub }}</span>
        <span class="block text-xs text-white/60">Nostr · {{ request.state === 'approved' ? 'Approved — connecting' : request.outbound ? 'Sent — awaiting approval' : 'Incoming — review request' }}</span>
      </button>
      <div v-if="loadingRequests && requestCount === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('common.loading') }}
      </div>
      <div v-else-if="requestCount === 0" class="p-4 text-center text-white/60 text-sm">
        {{ t('web5.noRequests') }}
      </div>
      <div v-else-if="loadingRequests" class="p-2 text-center text-white/45 text-xs">
        {{ t('common.loading') }}
      </div>
      <div
        v-for="req in connectionRequests"
        :key="req.id"
        class="p-3 bg-white/5 rounded-lg border-l-2 border-blue-500/50"
      >
        <div class="flex items-start justify-between gap-3">
          <div class="min-w-0 flex-1">
            <p class="text-xs font-mono text-white/70 truncate" :title="req.from_did">{{ peerNameFromPubkey(req.from_did) }}</p>
            <p v-if="req.message" class="text-sm text-white/80 mt-1 break-words">{{ req.message }}</p>
            <p class="text-xs text-white/40 mt-1">{{ formatMessageTime(req.created_at) }}</p>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <button
              @click="acceptRequest(req.id)"
              :disabled="processingRequestId === req.id"
              class="px-3 py-1.5 text-xs rounded-lg bg-green-500/20 text-green-400 hover:bg-green-500/30 transition-colors disabled:opacity-50"
            >
              {{ t('web5.accept') }}
            </button>
            <button
              @click="rejectRequest(req.id)"
              :disabled="processingRequestId === req.id"
              class="px-3 py-1.5 text-xs rounded-lg bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors disabled:opacity-50"
            >
              {{ t('web5.reject') }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="mt-auto shrink-0 pt-4 space-y-3">
      <!-- Always the bottom 50/50 pair, every screen size — consistent with
           the other web5 cards' full-width bottom actions. -->
      <div class="grid grid-cols-2 gap-3">
        <button
          @click="router.push('/dashboard/server/federation')"
          class="mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ t('web5.findNodes') }}
        </button>
        <button
          @click="refreshActiveTab"
          :disabled="loadingPeers || loadingRequests"
          class="mobile-card-action glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors"
        >
          {{ loadingPeers || loadingRequests ? t('common.loading') : t('common.refresh') }}
        </button>
      </div>
    </div>
  </div>

  <!-- Send Message Modal -->
  <Teleport to="body">
    <div v-if="showSendMessageModal" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="closeSendMessageModal()">
      <div ref="sendMessageModalRef" class="glass-card p-6 max-w-2xl w-full max-h-[90vh] overflow-y-auto">
        <h3 class="text-lg font-semibold text-white mb-4">{{ t('web5.sendMessageTitle') }}</h3>
        <p class="text-white/70 text-sm mb-4">Messages are sent over the Tor network to the selected peer.</p>
        <div class="space-y-4">
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('web5.to') }}</label>
            <select
              v-model="sendMessageTo"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
            >
              <option value="">{{ t('web5.selectPeer') }}</option>
              <option v-for="p in peers" :key="p.pubkey" :value="p.onion">
                {{ p.name || p.onion || (p.pubkey || '').slice(0, 12) + '...' }}
              </option>
            </select>
          </div>
          <div>
            <label class="block text-sm font-medium text-white/80 mb-2">{{ t('web5.message') }}</label>
            <textarea
              v-model="sendMessageText"
              rows="3"
              class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500"
              :placeholder="t('web5.messagePlaceholder')"
            ></textarea>
          </div>
        </div>
        <div class="flex gap-3 mt-6">
          <button
            @click="sendMessage"
            :disabled="!sendMessageTo || !sendMessageText.trim() || sendingMessage"
            class="flex-1 px-4 py-2 rounded-lg bg-orange-500 text-white font-medium hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {{ sendingMessage ? t('common.sending') : t('common.send') }}
          </button>
          <button
            @click="closeSendMessageModal()"
            class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors"
          >
            {{ t('common.cancel') }}
          </button>
        </div>
        <p v-if="sendMessageError" class="mt-3 text-sm text-red-400">{{ sendMessageError }}</p>
        <p v-if="sendMessageSuccess" class="mt-3 text-sm text-green-400">{{ sendMessageSuccess }}</p>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { useMessageToast } from '@/composables/useMessageToast'
import { useWeb5BadgeStore } from '@/stores/web5Badge'
import type { PendingPeerRequest } from '@/api/rpc-client'
import { availabilityRank, lastContactLabel, nodeAvailability, validContact, type ReachabilitySample } from './nodeAvailability'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import { formatMessageTime } from './utils'
import type { Peer, ConnectionRequest } from './types'

const router = useRouter()
const { t } = useI18n()
const messageToast = useMessageToast()
const web5Badge = useWeb5BadgeStore()


const CONNECTED_NODES_CACHE_KEY = 'archipelago.web5.connected-nodes.v2'
type ConnectedNodesCache = {
  peers: Peer[]
  observers: Peer[]
  peerReachable: Record<string, ReachabilitySample>
  connectionRequests: ConnectionRequest[]
}

function readConnectedNodesCache(): Partial<ConnectedNodesCache> {
  if (typeof window === 'undefined') return {}
  try {
    const raw = window.sessionStorage.getItem(CONNECTED_NODES_CACHE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as Partial<ConnectedNodesCache>
    return {
      peers: Array.isArray(parsed.peers) ? parsed.peers : [],
      observers: Array.isArray(parsed.observers) ? parsed.observers : [],
      peerReachable: parsed.peerReachable && typeof parsed.peerReachable === 'object' ? parsed.peerReachable : {},
      connectionRequests: Array.isArray(parsed.connectionRequests) ? parsed.connectionRequests : [],
    }
  } catch {
    return {}
  }
}

function writeConnectedNodesCache(state: ConnectedNodesCache) {
  if (typeof window === 'undefined') return
  try {
    window.sessionStorage.setItem(CONNECTED_NODES_CACHE_KEY, JSON.stringify(state))
  } catch {
    // Cache is best-effort.
  }
}

const nodesContainerTab = ref<'trusted' | 'observers' | 'requests'>('trusted')
const { loadReceivedMessages } = messageToast

const cached = readConnectedNodesCache()
const peers = ref<Peer[]>(cached.peers ?? [])
const observers = ref<Peer[]>(cached.observers ?? [])
const loadingPeers = ref(false)
const peerReachableLocal = ref<Record<string, ReachabilitySample>>(cached.peerReachable ?? {})
const now = ref(Date.now())
let ageTimer: ReturnType<typeof setInterval> | undefined
let peerGeneration = 0
onMounted(() => { ageTimer = setInterval(() => { now.value = Date.now() }, 15_000) })
onUnmounted(() => { clearInterval(ageTimer); peerGeneration++ })
function availability(peer: Peer) { return nodeAvailability(peerReachableLocal.value[peer.onion], now.value) }
function availabilityText(peer: Peer) {
  const status = availability(peer)
  return status === 'online' ? 'Online' : status === 'unreachable' ? 'Unreachable' : 'Status unknown'
}
function contactText(peer: Peer) {
  const local = validContact(peerReachableLocal.value[peer.onion]?.lastSeen, now.value) ?? 0
  const recorded = validContact(peer.last_seen, now.value) ?? 0
  return lastContactLabel(Math.max(local, recorded), now.value)
}
function statusClass(peer: Peer) {
  return availability(peer) === 'online' ? 'bg-green-400' : availability(peer) === 'unreachable' ? 'bg-amber-400' : 'bg-white/30'
}
const sortedPeers = computed(() => [...peers.value].sort((a, b) => availabilityRank(availability(a)) - availabilityRank(availability(b))))
const sortedObservers = computed(() => [...observers.value].sort((a, b) => availabilityRank(availability(a)) - availabilityRank(availability(b))))

// Send message modal
const showSendMessageModal = ref(false)
const sendMessageModalRef = ref<HTMLElement | null>(null)
const sendMessageRestoreFocusRef = ref<HTMLElement | null>(null)
function closeSendMessageModal() {
  sendMessageRestoreFocusRef.value?.focus?.()
  showSendMessageModal.value = false
}
useModalKeyboard(sendMessageModalRef, showSendMessageModal, closeSendMessageModal, { restoreFocusRef: sendMessageRestoreFocusRef })
const sendMessageTo = ref('')
const sendMessageText = ref('')
const sendingMessage = ref(false)
const sendMessageError = ref('')
const sendMessageSuccess = ref('')

// Connection requests
const connectionRequests = ref<ConnectionRequest[]>(cached.connectionRequests ?? [])
const nostrRequests = ref<PendingPeerRequest[]>([])
const requestCount = computed(() => connectionRequests.value.length + nostrRequests.value.length)
const requestError = ref('')
const peersError = ref('')
const loadingRequests = ref(false)
const processingRequestId = ref<string | null>(null)

const emit = defineEmits<{
  toast: [text: string]
}>()

function peerNameFromPubkey(pubkey: string): string {
  const peer = [...peers.value, ...observers.value].find(p => p.pubkey === pubkey || p.onion === pubkey)
  if (peer?.name) return peer.name
  return (pubkey || '').slice(0, 16) + '...'
}

type FederationNode = Awaited<ReturnType<typeof rpcClient.federationListNodes>>['nodes'][number]

function federationNodeToPeer(node: FederationNode): Peer {
  return {
    onion: node.onion,
    pubkey: node.pubkey,
    did: node.did,
    last_seen: node.last_seen,
    name: node.name || `Federation: ${node.did?.slice(0, 16) || 'node'}`,
  }
}

function switchToRequestsTab() {
  nodesContainerTab.value = 'requests'
  if (!loadingRequests.value) {
    loadConnectionRequests()
  }
}

// The single bottom Refresh acts on whichever tab is open.
function refreshActiveTab() {
  if (nodesContainerTab.value === 'requests') void loadConnectionRequests()
  else void loadPeers()
}

function persistCache() {
  writeConnectedNodesCache({ peers: peers.value, observers: observers.value, peerReachable: peerReachableLocal.value, connectionRequests: connectionRequests.value })
}
async function loadPeers() {
  if (loadingPeers.value) return
  const generation = ++peerGeneration
  loadingPeers.value = true
  peersError.value = ''
  let direct: Peer[] | undefined
  let federated: FederationNode[] | undefined
  function applyLists() {
    if (generation !== peerGeneration) return
    const trusted = direct ? [...direct] : [...peers.value]
    const observerList: Peer[] = federated ? [] : [...observers.value]
    for (const node of federated ?? []) {
      if (!node.onion || node.trust_level === 'untrusted') continue
      const list = node.trust_level === 'observer' ? observerList : trusted
      const peer = federationNodeToPeer(node)
      const index = list.findIndex(p => p.did === node.did || p.onion === node.onion || (p.pubkey && p.pubkey === node.pubkey))
      if (index < 0) list.push(peer)
      else list[index] = { ...list[index], ...peer }
    }
    peers.value = trusted
    observers.value = observerList
    persistCache()
  }
  const results = await Promise.allSettled([
    rpcClient.listPeers().then(result => { direct = result.peers || []; applyLists() }),
    rpcClient.federationListNodes().then(result => { federated = result.nodes || []; applyLists() }),
  ])
  if (generation !== peerGeneration) return
  if (results.some(result => result.status === 'rejected')) peersError.value = 'Some nodes could not be refreshed. Showing available information.'
  loadingPeers.value = false
  // Bound concurrent probes, render each result immediately, and ignore replies
  // from a superseded refresh. A failed RPC is unknown, not an offline node.
  const pending = [...new Map([...peers.value, ...observers.value].map(peer => [peer.onion, peer])).values()]
  async function probe() {
    while (pending.length && generation === peerGeneration) {
      const peer = pending.shift()!
      let reachable: boolean | null = null
      try { reachable = (await rpcClient.checkPeerReachable(peer.onion)).reachable } catch { /* unknown */ }
      if (generation !== peerGeneration) return
      now.value = Date.now()
      peerReachableLocal.value[peer.onion] = {
        reachable,
        checkedAt: now.value,
        lastSeen: reachable ? now.value : validContact(peerReachableLocal.value[peer.onion]?.lastSeen, now.value),
      }
      persistCache()
    }
  }
  void Promise.all(Array.from({ length: Math.min(4, pending.length) }, probe))
}

async function sendMessage() {
  if (!sendMessageTo.value || !sendMessageText.value.trim()) return
  sendingMessage.value = true
  sendMessageError.value = ''
  sendMessageSuccess.value = ''
  try {
    await rpcClient.sendMessageToPeer(sendMessageTo.value, sendMessageText.value.trim())
    sendMessageSuccess.value = t('web5.messageSent')
    sendMessageText.value = ''
    setTimeout(() => {
      showSendMessageModal.value = false
      sendMessageSuccess.value = ''
    }, 1500)
  } catch (e) {
    sendMessageError.value = e instanceof Error ? e.message : t('web5.failedToSend')
  } finally {
    sendingMessage.value = false
  }
}

async function loadConnectionRequests() {
  if (loadingRequests.value) return
  loadingRequests.value = true
  requestError.value = ''
  const results = await Promise.allSettled([
    rpcClient.call<{ requests: ConnectionRequest[] }>({ method: 'network.list-requests' }).then(res => { connectionRequests.value = res.requests || [] }),
    rpcClient.federationListPendingRequests().then(res => {
      nostrRequests.value = (res.requests || []).filter(request => request.state === 'pending' || request.state === 'sent' ||
        (request.state === 'approved' && ![...peers.value, ...observers.value].some(peer => peer.did === request.from_did)))
    }),
  ])
  if (results.some(result => result.status === 'rejected')) requestError.value = 'Some requests could not be refreshed. Try again.'
  web5Badge.pendingRequestCount = connectionRequests.value.length + nostrRequests.value.filter(request => request.state === 'pending' && !request.outbound).length
  persistCache()
  loadingRequests.value = false
}

async function acceptRequest(requestId: string) {
  processingRequestId.value = requestId
  try {
    await rpcClient.call({ method: 'network.accept-request', params: { id: requestId } })
    connectionRequests.value = connectionRequests.value.filter(r => r.id !== requestId)
    web5Badge.pendingRequestCount = connectionRequests.value.length + nostrRequests.value.filter(request => request.state === 'pending' && !request.outbound).length
    writeConnectedNodesCache({
      peers: peers.value,
      observers: observers.value,
      peerReachable: peerReachableLocal.value,
      connectionRequests: connectionRequests.value,
    })
    await loadPeers()
    emit('toast', t('web5.connectionAccepted'))
  } catch {
    emit('toast', t('web5.failedToAcceptRequest'))
  } finally {
    processingRequestId.value = null
  }
}

async function rejectRequest(requestId: string) {
  processingRequestId.value = requestId
  try {
    await rpcClient.call({ method: 'network.reject-request', params: { id: requestId } })
    connectionRequests.value = connectionRequests.value.filter(r => r.id !== requestId)
    web5Badge.pendingRequestCount = connectionRequests.value.length + nostrRequests.value.filter(request => request.state === 'pending' && !request.outbound).length
    writeConnectedNodesCache({
      peers: peers.value,
      observers: observers.value,
      peerReachable: peerReachableLocal.value,
      connectionRequests: connectionRequests.value,
    })
    emit('toast', t('web5.requestRejected'))
  } catch {
    emit('toast', t('web5.failedToRejectRequest'))
  } finally {
    processingRequestId.value = null
  }
}

defineExpose({ loadPeers, loadReceivedMessages, loadConnectionRequests, peers, observers })
</script>
