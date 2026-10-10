<script setup lang="ts">
import { ref, computed, watch, nextTick, onActivated, onDeactivated, onMounted, onUnmounted } from 'vue'
import { useRoute } from 'vue-router'
import { useViewActive } from '@/composables/useViewActive'
import { useMeshStore } from '@/stores/mesh'
import { useTransportStore } from '@/stores/transport'
import type { MeshMessage, MeshPeer, SessionStatus } from '@/stores/mesh'
import AnimatedLogo from '@/components/AnimatedLogo.vue'
import MeshMap from '@/components/MeshMap.vue'
import MeshBitcoinPanel from '@/views/mesh/MeshBitcoinPanel.vue'
import MeshDeadmanPanel from '@/views/mesh/MeshDeadmanPanel.vue'
import MeshDevicePanel from '@/views/mesh/MeshDevicePanel.vue'
import MeshAssistantPanel from '@/views/mesh/MeshAssistantPanel.vue'
import HopVizModal from '@/views/mesh/HopVizModal.vue'
import { rpcClient } from '@/api/rpc-client'
import { appConfirm } from '@/composables/useAppConfirm'
import { wsClient } from '@/api/websocket'
import { IMAGE_COMPRESSION_PRESETS, compressImage, makeThumbnail, type ImageCompressionPreset } from '@/utils/imageCompression'
import MediaLightbox from '@/components/cloud/MediaLightbox.vue'
import type { FileBrowserItem } from '@/api/filebrowser-client'
import { useCachedResource } from '@/composables/useCachedResource'
import RefreshIndicator from '@/components/RefreshIndicator.vue'
import { channelContactId, isChannelMessage, isPublicChannelMessage } from '@/views/mesh/channelMessages'
import '@/views/mesh/mesh-styles.css'

const mesh = useMeshStore()
const transport = useTransportStore()
const route = useRoute()

// Everything this view Teleports to <body> — the mobile tab bar, the chat back
// button, and the two transport choosers — lives OUTSIDE the view's own DOM
// subtree, so KeepAlive deactivating this view does not remove it. Without this
// gate the tab bar and back button stayed pinned above the bottom bar on every
// other screen. Gating the Teleport (not its child) means nothing reaches
// <body> at all while Mesh is off screen; the view itself stays alive, so
// revisiting it is still instant.
const isViewActive = useViewActive()

// "Flash LoRa" header button target: the connected radio wins, else any
// detected-but-unconnected stick. Opens the global setup modal at its
// flash step (backend stops the listener and frees the port itself).
const flashLoraPath = computed(() =>
  (mesh.status?.device_connected && mesh.status.device_path) ||
  mesh.status?.detected_devices?.[0] ||
  ''
)
function openFlashLora() {
  if (flashLoraPath.value) mesh.openFlashFlow(flashLoraPath.value)
}

// Responsive layout breakpoints
const isWideDesktop = ref(window.innerWidth >= 1536)
const isVeryWideDesktop = ref(window.innerWidth >= 2560 && window.innerHeight >= 1200)
const isMobile = ref(window.innerWidth < 1280)

function handleResize() {
  isWideDesktop.value = window.innerWidth >= 1536
  isVeryWideDesktop.value = window.innerWidth >= 2560 && window.innerHeight >= 1200
  isMobile.value = window.innerWidth < 1280
}

// Active chat: either a peer or a channel
const activeChatPeer = ref<MeshPeer | null>(null)
const activeChatChannel = ref<{ index: number; name: string } | null>(null)
const messageText = ref('')
const sendError = ref('')
const broadcasting = ref(false)
const broadcastResult = ref<string | null>(null) // 'ok' | error message
const refreshing = ref(false)
const configuring = ref(false)
const connectingDevice = ref<string | null>(null)
// Device-detected onboarding now lives in the global MeshDeviceSetupModal (App.vue).
const chatScrollEl = ref<HTMLElement | null>(null)
const messageInputRef = ref<HTMLInputElement | null>(null)
const mobileShowChat = ref(false)
// Device status panel starts collapsed on mobile (expandable via its header).
const deviceExpanded = ref(false)
let pollInterval: ReturnType<typeof setInterval> | null = null
let wsUnsub: (() => void) | null = null

// The Public channel (always available on Meshcore)
// "Public" is the stock MeshCore/Meshtastic public channel in slot 0.
const publicChannel = { index: 0, name: 'Public' }

const publicChannelUnread = computed(() => {
  const contactIds = new Set([
    channelContactId(publicChannel.index),
    ...mesh.messages.filter(isPublicChannelMessage).map(message => message.peer_contact_id),
  ])
  return Object.entries(mesh.unreadCounts)
    .filter(([contactId]) => contactIds.has(Number(contactId)))
    .reduce((total, [, count]) => total + count, 0)
})

// Archipelago Channel — Tor-based messaging to all federated/peered nodes
const archChannelActive = ref(false)
const archMessages = ref<Array<{ from_pubkey: string; from_name?: string; message: string; timestamp: string; direction?: string }>>([])
const archUnread = ref(0)
let archPollInterval: ReturnType<typeof setInterval> | null = null
// Federation node name cache: pubkey -> node name (legacy, kept for archMessages display)
const fedNodeNames = ref<Record<string, string>>({})

// Federation node enrichment cache, keyed by DID. Used by mergedPeers to
// upgrade radio-discovered mesh peers with the canonical server name and
// nostr identity (npub) reported by federation.list-nodes.
interface FedNodeInfo {
  did: string
  name: string | null
  pubkey: string
  onion: string
  npub: string | null
}
const fedNodesByDid = ref<Map<string, FedNodeInfo>>(new Map())
// Our own onion / DID / mesh advert name — used to filter "self" out of
// the merged peer list. Without these filters every node sees itself as a
// duplicate row (federation lists carry a self-entry, and the meshcore
// radio occasionally surfaces its own outgoing advert as a peer).
const selfTorOnion = ref<string | null>(null)
const selfDid = ref<string | null>(null)

// User-set aliases for peers, keyed by whichever identifier is most stable
// for that peer (DID first, then mesh pubkey_hex, then federation pubkey).
// Loaded from `mesh.contacts-list` on mount and refreshed on every save so
// the rename propagates everywhere display_name is computed.
const contactAliases = ref<Map<string, string>>(new Map())
async function refreshContacts() {
  try {
    const res = await rpcClient.meshContactsList()
    const next = new Map<string, string>()
    for (const c of res.contacts) {
      if (c.alias && c.alias.trim()) next.set(c.pubkey, c.alias.trim())
    }
    contactAliases.value = next
  } catch { /* non-fatal */ }
}
function aliasFor(mp: { did: string | null; primary_pubkey_hex: string | null }): string | null {
  if (mp.did && contactAliases.value.has(mp.did)) return contactAliases.value.get(mp.did) ?? null
  if (mp.primary_pubkey_hex && contactAliases.value.has(mp.primary_pubkey_hex)) {
    return contactAliases.value.get(mp.primary_pubkey_hex) ?? null
  }
  return null
}
async function refreshFederationNodes() {
  try {
    const res = await rpcClient.federationListNodes()
    const next = new Map<string, FedNodeInfo>()
    for (const n of res.nodes) {
      next.set(n.did, {
        did: n.did,
        name: n.name ?? n.last_state?.node_name ?? null,
        pubkey: n.pubkey,
        onion: n.onion,
        npub: n.last_state?.nostr_npub ?? null,
      })
    }
    fedNodesByDid.value = next
    mesh.updateFederatedPositions(res.nodes)
  } catch { /* non-fatal */ }
}
async function refreshSelfOnion() {
  try {
    const res = await rpcClient.getTorAddress()
    selfTorOnion.value = (res.tor_address ?? '').replace(/\.onion\/?$/, '').replace(/^https?:\/\//, '')
  } catch { /* non-fatal */ }
}
async function refreshSelfDid() {
  try {
    const res = await rpcClient.getNodeDid()
    selfDid.value = res.did || null
  } catch { /* non-fatal */ }
}

async function openArchChannel() {
  activeChatPeer.value = null
  activeChatChannel.value = null
  archChannelActive.value = true
  archUnread.value = 0
  mobileShowChat.value = true
  // Load federation node names for resolving pubkeys to names
  try {
    const res = await rpcClient.federationListNodes()
    const names: Record<string, string> = {}
    for (const node of res.nodes) {
      if (node.pubkey) names[node.pubkey] = node.name || node.did.slice(0, 12) + '...'
    }
    fedNodeNames.value = names
  } catch { /* non-fatal */ }
  loadArchMessages()
  if (!archPollInterval) {
    archPollInterval = setInterval(loadArchMessages, 15000)
  }
}

async function loadArchMessages() {
  try {
    const res = await rpcClient.getReceivedMessages()
    const newMessages = res.messages || []
    // Track unread: count new received messages since last load
    if (archMessages.value.length > 0 && !archChannelActive.value) {
      const newReceived = newMessages.filter(
        m => m.direction !== 'sent' && m.from_pubkey !== 'me'
          && !archMessages.value.some(existing =>
            existing.from_pubkey === m.from_pubkey && existing.timestamp === m.timestamp
          )
      )
      archUnread.value += newReceived.length
    }
    archMessages.value = newMessages
  } catch { /* silent */ }
}

const sendingArch = ref(false)

async function sendArchMessage() {
  if (!messageText.value.trim()) return
  sendError.value = ''
  sendingArch.value = true
  try {
    const nodes = await rpcClient.federationListNodes()
    // Get our own onion address to skip sending to self
    let selfOnion: string | null = null
    try {
      const tor = await rpcClient.getTorAddress()
      selfOnion = tor.tor_address
    } catch { /* non-fatal */ }
    const msg = messageText.value.trim()
    const targets = nodes.nodes
      .map((node) => node.onion || node.did)
      // Skip sending to ourselves (would create a duplicate received message).
      .filter(
        (nodeOnion) =>
          !(selfOnion &&
            (nodeOnion === selfOnion ||
              nodeOnion === selfOnion.replace('.onion', '') ||
              selfOnion === nodeOnion + '.onion'))
      )
    // Send to all peers CONCURRENTLY so the spinner clears after the slowest
    // single delivery (one Tor round-trip) rather than the sum of all of them —
    // previously a slow or offline node kept the "sending" spinner up long after
    // the online peers had already received the message.
    const results = await Promise.allSettled(
      targets.map((nodeOnion) => rpcClient.sendMessageToPeer(nodeOnion, msg))
    )
    const sent = results.filter((r) => r.status === 'fulfilled').length
    try {
      await rpcClient.call({ method: 'node-store-sent', params: { message: msg } })
    } catch { /* non-fatal */ }
    messageText.value = ''
    if (sent === 0 && nodes.nodes.length <= 1) sendError.value = 'No other peers in federation — add nodes first'
    await loadArchMessages()
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'Send failed'
  } finally {
    sendingArch.value = false
  }
}

const togglingOffGrid = ref(false)
const peerSessionInfo = ref<SessionStatus | null>(null)
const rotatingPrekeys = ref(false)
const outboxCount = ref(0)
async function handleRotatePrekeys() {
  if (rotatingPrekeys.value) return
  rotatingPrekeys.value = true
  try {
    await mesh.rotatePrekeys()
    if (activeChatPeer.value) {
      peerSessionInfo.value = await mesh.getSessionStatus(activeChatPeer.value.contact_id)
    }
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'rotate failed'
  } finally {
    rotatingPrekeys.value = false
  }
}
async function refreshOutboxCount() {
  try {
    const resp = await mesh.getOutbox()
    outboxCount.value = resp?.count ?? 0
  } catch {
    outboxCount.value = 0
  }
}

async function clearAllMesh() {
  const ok = await appConfirm({
    title: 'Clear mesh data',
    message: 'Clear all mesh peers, messages, and chat history? This cannot be undone.',
    confirmLabel: 'Clear everything',
    danger: true,
  })
  if (!ok) return
  try {
    await rpcClient.call({ method: 'mesh.clear-all' })
    await mesh.refreshAll()
    activeChatPeer.value = null
  } catch (e) {
    console.error('Failed to clear mesh:', e)
  }
}

// Phase 4: Off-grid Bitcoin + Dead Man's Switch
const activeTab = ref<'chat' | 'bitcoin' | 'deadman' | 'assistant' | 'map' | 'device'>('chat')

// Tools tab for 3rd column on wide desktop and mobile below-chat
const toolsTab = ref<'bitcoin' | 'deadman' | 'assistant' | 'map' | 'device'>('bitcoin')

// Mobile: a single set of floating tabs drives the whole pane (Chat + tools).
// 'chat' shows the peers list / active conversation; the rest swap the pane to
// that tool. Selecting a tool leaves any open conversation.
const mobileTab = ref<'chat' | 'bitcoin' | 'deadman' | 'assistant' | 'map' | 'device'>('chat')
function selectMobileTab(tab: 'chat' | 'bitcoin' | 'deadman' | 'assistant' | 'map' | 'device') {
  mobileTab.value = tab
  if (tab !== 'chat') mobileShowChat.value = false
}

// Panel visibility computeds
const showChatPanel = computed(() =>
  activeTab.value === 'chat' || isWideDesktop.value || (isMobile.value && mobileShowChat.value)
)
const showBitcoinPanel = computed(() => {
  if (isWideDesktop.value) return toolsTab.value === 'bitcoin'
  if (isMobile.value) return mobileTab.value === 'bitcoin'
  return activeTab.value === 'bitcoin'
})
const showDeadmanPanel = computed(() => {
  if (isWideDesktop.value) return toolsTab.value === 'deadman'
  if (isMobile.value) return mobileTab.value === 'deadman'
  return activeTab.value === 'deadman'
})
const showAssistantPanel = computed(() => {
  if (isWideDesktop.value) return toolsTab.value === 'assistant'
  if (isMobile.value) return mobileTab.value === 'assistant'
  return activeTab.value === 'assistant'
})
const showMapPanel = computed(() => {
  if (isWideDesktop.value) return toolsTab.value === 'map'
  if (isMobile.value) return mobileTab.value === 'map'
  return activeTab.value === 'map'
})
const showDevicePanel = computed(() => {
  if (isWideDesktop.value) return toolsTab.value === 'device'
  if (isMobile.value) return mobileTab.value === 'device'
  return activeTab.value === 'device'
})
// Mobile: tool pane shows whenever a non-chat tab is active.
const showMobileTools = computed(() => isMobile.value && mobileTab.value !== 'chat')
const showTabBar = computed(() => !isWideDesktop.value && !isMobile.value)

// Fetch session status when active peer changes
watch(() => activeChatPeer.value, async (peer) => {
  if (peer) {
    try {
      peerSessionInfo.value = await mesh.getSessionStatus(peer.contact_id)
    } catch {
      peerSessionInfo.value = null
    }
    scheduleReadReceipt()
  } else {
    peerSessionInfo.value = null
  }
})

async function handleToggleOffGrid() {
  togglingOffGrid.value = true
  try {
    await transport.setMeshOnly(!transport.meshOnly)
  } finally { togglingOffGrid.value = false }
}

// Track the on-screen keyboard height (mobile) so the conversation pane + back
// button can sit just above it — fixed elements ignore the keyboard otherwise
// and the input ends up hidden behind it. Publishes --keyboard-inset on <html>.
function updateKeyboardInset() {
  const vv = window.visualViewport
  if (!vv) return
  const inset = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop))
  document.documentElement.style.setProperty('--keyboard-inset', `${inset}px`)
}

// ── Cached tab-entry fan-out (PERF-02, T-02-01, T-02-13, T-02-16) ──────────
//
// The six fetch groups armMeshLive() fires on every tab entry are wrapped in
// useCachedResource entries so a revisit inside each group's TTL issues no
// RPC at all, while a stale revisit revalidates in the background (driven
// through the shared RefreshIndicator below) without clearing the screen.
//
// Placement: mesh.refreshAll() and transport.fetchStatus() are store actions
// with other callers (clearAllMesh() below, and setMeshOnly()/
// Web5SendReceiveModals.vue's pre-send balance check, respectively) that
// need a guaranteed-fresh read, not a cache-gated one — so those two store
// actions themselves are left untouched, and the useCachedResource() call
// that wraps each of them lives here instead of inside stores/mesh.ts or
// stores/transport.ts. This is a deliberate technical choice, not a
// re-reading of the plan's intent: Pinia's `defineStore(id, setup)` runs its
// setup inside a bare `effectScope`, not a real component instance, and
// useCachedResource's internal `onActivated()` registration is a documented
// Vue no-op (with a dev warning) outside a component's own setup() — hosting
// it at store scope would compile but would never actually revalidate
// anything on tab reactivation. Mesh.vue is the one call site that
// legitimately owns the KeepAlive/component lifecycle, so it hosts all six
// resources; the store-owned groups' fetchers still just re-invoke the
// store's existing action, so the store's real refresh path is what is
// genuinely being cached and gated.
//
// Every entry is `immediate: false` and force-refreshed only when stale via
// refreshMeshGroupIfStale() below, called from a single Promise.allSettled
// array in armMeshLive — never auto-triggered independently — so the
// cold-load fan-out stays concurrent instead of becoming a serialized chain
// of six separately-awaited refreshes (T-02-16), and one rejected group
// never suppresses the other five. Each fetcher resolves to a plain
// timestamp (not the real payload): the real UI-facing state continues to
// flow through mesh.status/mesh.peers/etc (set by the wrapped store
// actions/view-local refreshers as a side effect, exactly as before this
// plan) — these resources exist purely to gate/dedupe the RPC and to expose
// a `loadState` for the shared RefreshIndicator, not to hold the data itself.
const meshRefreshResource = useCachedResource<number>({
  key: 'mesh.refresh-all',
  fetcher: async (signal) => {
    void signal
    await mesh.refreshAll()
    return Date.now()
  },
  // Peer/status/message data is exactly the liveness-critical reachability
  // surface T-02-13 forbids freezing — shortest TTL of the six groups.
  ttlMs: 10_000,
  persist: false, // mesh.peers carries peer DIDs/pubkeys — identity payload
  immediate: false,
})
const transportStatusResource = useCachedResource<number>({
  key: 'mesh.transport-status',
  fetcher: async (signal) => {
    void signal
    await transport.fetchStatus()
    return Date.now()
  },
  ttlMs: 10_000, // transport availability / mesh-only mode is live state
  persist: true, // aggregate status only — no peer identity payload
  immediate: false,
})
const federationNodesResource = useCachedResource<number>({
  key: 'mesh.federation-nodes',
  fetcher: async (signal) => {
    void signal
    await refreshFederationNodes()
    return Date.now()
  },
  ttlMs: 30_000, // D-06 default — federation node list isn't liveness-critical
  persist: false, // federation nodes carry DID/pubkey/onion — identity payload
  immediate: false,
})
const selfOnionResource = useCachedResource<number>({
  key: 'mesh.self-onion',
  fetcher: async (signal) => {
    void signal
    await refreshSelfOnion()
    return Date.now()
  },
  ttlMs: 300_000, // this node's own onion address is effectively static
  persist: false, // this node's own onion address — identity payload
  immediate: false,
})
const selfDidResource = useCachedResource<number>({
  key: 'mesh.self-did',
  fetcher: async (signal) => {
    void signal
    await refreshSelfDid()
    return Date.now()
  },
  ttlMs: 300_000, // this node's own DID is effectively static
  persist: false, // this node's own DID — identity payload
  immediate: false,
})
const contactsResource = useCachedResource<number>({
  key: 'mesh.contacts',
  fetcher: async (signal) => {
    void signal
    await refreshContacts()
    return Date.now()
  },
  ttlMs: 30_000, // user-set aliases change rarely; the D-06 default is enough
  persist: false, // contact records/aliases — identity payload per T-02-01
  immediate: false,
})

const meshCachedGroups = [
  meshRefreshResource, transportStatusResource, federationNodesResource,
  selfOnionResource, selfDidResource, contactsResource,
]
// Drives the header RefreshIndicator (D-05): visible whenever any of the six
// groups is revalidating in the background, including peer reachability, so
// a resumed tab never presents a frozen reachability state as current
// (T-02-13).
const meshRefreshIndicatorState = computed(() =>
  meshCachedGroups.some((r) => r.loadState.value === 'refreshing') ? 'refreshing' : 'ready'
)

/** Force a group's refresh only when it has never resolved or is past its
 * own TTL — mirrors Cloud.vue's loadCounts() gate. Deliberately synchronous
 * up to the point of kicking off `refresh()`, so all six calls in
 * armMeshLive's Promise.allSettled array start in the same tick rather than
 * forming a waterfall. */
function refreshMeshGroupIfStale(res: (typeof meshCachedGroups)[number]): Promise<void> {
  if (res.entry.data === null || res.isStale.value) return res.refresh()
  return Promise.resolve()
}

// Mesh is a live communications surface — peer reachability, messages and
// status are exactly the kind of data the threat model (T-02-13) forbids
// rendering from a paused cache without revalidating. Nothing here is
// once-per-session: window/document listeners are only-while-visible, and
// the data loads + poll intervals + ws subscription are every-entry. Every
// arm is idempotent (existing handles cleared/removed first), so two
// consecutive activations never double-arm.
//
// Called from BOTH onMounted and onActivated: onActivated is a no-op outside
// a <KeepAlive> boundary (Vue only auto-fires it on first mount for a
// component that already has a KeepAlive ancestor), so a bare mount (a unit
// test, or any future non-KeepAlive usage) must not silently skip every
// timer/subscription/listener this view owns. The harmless extra pass
// onActivated makes on a KeepAlive-wrapped first mount is absorbed by the
// idempotent arm logic below.
function armMeshLive() {
  window.removeEventListener('resize', handleResize)
  window.addEventListener('resize', handleResize)
  handleResize() // window size may have changed while off screen

  document.removeEventListener('pointerdown', handleDocClickForMenu)
  document.addEventListener('pointerdown', handleDocClickForMenu)
  document.removeEventListener('pointerdown', closeAttachMenuOnOutsideClick)
  document.addEventListener('pointerdown', closeAttachMenuOnOutsideClick)

  window.removeEventListener('archipelago:share-to-mesh', loadPendingFromSession)
  window.addEventListener('archipelago:share-to-mesh', loadPendingFromSession)

  if (window.visualViewport) {
    window.visualViewport.removeEventListener('resize', updateKeyboardInset)
    window.visualViewport.removeEventListener('scroll', updateKeyboardInset)
    window.visualViewport.addEventListener('resize', updateKeyboardInset)
    window.visualViewport.addEventListener('scroll', updateKeyboardInset)
    updateKeyboardInset()
  }

  // Direct navigation to /mesh (router.push, not the same-page custom event
  // above) is the only path that delivers a share-to-mesh handoff for a tab
  // that is already kept alive — App.vue only dispatches the custom event
  // when already on /mesh, so this explicit read must run on every
  // activation, not just the first mount, or a share arriving after the
  // first visit would silently never be picked up.
  loadPendingFromSession()

  // Cache-gated fan-out (PERF-02): each group only issues an RPC if it has
  // never resolved or is past its own TTL (refreshMeshGroupIfStale), and all
  // six start in the same Promise.allSettled array so a revisit inside every
  // group's TTL issues zero RPCs, a stale revisit revalidates concurrently
  // rather than as a serial chain, and one rejected group never blocks the
  // other five (T-02-16).
  void Promise.allSettled(
    meshCachedGroups.map((res) => refreshMeshGroupIfStale(res))
  ).then(() => {
    refreshOutboxCount()
    // Deep-link from a message toast: open the sender's conversation if we can
    // match a LoRa mesh peer; otherwise open the Archipelago channel, since
    // getReceivedMessages() (the toast's only message source) is exclusively
    // Archipelago (Tor federation) traffic — a federation pubkey will never
    // match an entry in mesh.peers, so without this fallback the deep-link
    // silently failed and just landed on the bare mesh page every time.
    const targetPeer = typeof route.query.peer === 'string' ? route.query.peer : ''
    const targetContact = typeof route.query.contact === 'string'
      ? Number(route.query.contact)
      : NaN
    if (Number.isInteger(targetContact)) {
      const match = mesh.peers.find(p => p.contact_id === targetContact)
      if (match) openChat(match)
    } else if (targetPeer) {
      const match = mesh.peers.find(
        (p) => p.pubkey_hex === targetPeer || p.did === targetPeer
      )
      if (match) {
        openChat(match)
      } else {
        openArchChannel()
      }
    }
  })

  // Start background polling for Archipelago (Tor) messages so unread count works
  loadArchMessages()
  if (archPollInterval) clearInterval(archPollInterval)
  archPollInterval = setInterval(loadArchMessages, 15000)

  if (pollInterval) clearInterval(pollInterval)
  let tick = 0
  pollInterval = setInterval(() => {
    mesh.fetchStatus()
    mesh.fetchPeers()
    mesh.fetchMessages()
    mesh.fetchDeadmanStatus()
    mesh.fetchBlockHeaders()
    // Contacts/aliases, federation nodes and the outbox badge previously
    // loaded ONCE at mount and went permanently stale — new federation
    // peers or renames never appeared without a full page reload. Every
    // 6th tick (~30s) keeps them fresh without adding per-5s load.
    if (++tick % 6 === 0) {
      void refreshContacts()
      void refreshFederationNodes()
      void refreshOutboxCount()
    }
  }, 5000)

  // Instant peer updates (#48): the backend nudges the data-model revision when
  // it discovers/updates a mesh peer, so refetch peers on the WS push rather
  // than waiting for the next 5s poll tick. The poll above stays as a backstop
  // (e.g. for peers going offline, which isn't pushed).
  if (wsUnsub) wsUnsub()
  wsUnsub = wsClient.subscribe(() => {
    mesh.fetchPeers()
  })
}
// Vue fires onActivated immediately after onMounted on a KeepAlive-wrapped
// component's first mount, which would otherwise double the six-way
// Promise.all + interval/subscription arm on every fresh page load. This
// flag lets onMounted's call count as the first activation's arm, so
// onActivated only re-arms on a genuine later reactivation (after a real
// deactivate), not redundantly right after mount.
let meshFreshMount = true
onActivated(() => {
  if (meshFreshMount) { meshFreshMount = false; return }
  armMeshLive()
})
onMounted(() => armMeshLive())

function teardownMeshLiveEffects() {
  window.removeEventListener('resize', handleResize)
  document.removeEventListener('pointerdown', handleDocClickForMenu)
  document.removeEventListener('pointerdown', closeAttachMenuOnOutsideClick)
  window.removeEventListener('archipelago:share-to-mesh', loadPendingFromSession)
  if (window.visualViewport) {
    window.visualViewport.removeEventListener('resize', updateKeyboardInset)
    window.visualViewport.removeEventListener('scroll', updateKeyboardInset)
  }
  document.documentElement.style.removeProperty('--keyboard-inset')
  if (pollInterval) { clearInterval(pollInterval); pollInterval = null }
  if (archPollInterval) { clearInterval(archPollInterval); archPollInterval = null }
  if (wsUnsub) { wsUnsub(); wsUnsub = null }
}

onDeactivated(() => teardownMeshLiveEffects())
onUnmounted(() => teardownMeshLiveEffects())

// Active chat name for the header
const activeChatName = computed(() => {
  if (archChannelActive.value) return 'Archipelago'
  if (activeChatChannel.value) return activeChatChannel.value.name
  if (activeMergedPeer.value) return activeMergedPeer.value.display_name
  if (activeChatPeer.value) return activeChatPeer.value.advert_name
  return ''
})

const activeChatSub = computed(() => {
  if (archChannelActive.value) return 'All nodes over Tor'
  if (activeChatChannel.value) return 'Mesh radio'
  const merged = activeMergedPeer.value
  if (merged) {
    const parts: string[] = []
    if (merged.short_did) parts.push(merged.short_did)
    else if (merged.primary_pubkey_hex) parts.push(truncatePubkey(merged.primary_pubkey_hex))
    if (merged.npub) parts.push(`${merged.npub.slice(0, 12)}…${merged.npub.slice(-6)}`)
    return parts.join(' · ')
  }
  if (activeChatPeer.value) return truncatePubkey(activeChatPeer.value.pubkey_hex)
  return ''
})

const hasActiveChat = computed(() => !!activeChatPeer.value || !!activeChatChannel.value || archChannelActive.value)

// Messages filtered to the active chat
const chatMessages = computed(() => {
  if (archChannelActive.value) {
    return archMessages.value.map((m, i) => {
      const isSent = m.direction === 'sent' || m.from_pubkey === 'me'
      let peerName = 'Unknown'
      if (isSent) {
        peerName = 'You'
      } else if (m.from_name) {
        peerName = m.from_name
      } else if (fedNodeNames.value[m.from_pubkey]) {
        peerName = fedNodeNames.value[m.from_pubkey]!
      } else {
        peerName = m.from_pubkey.slice(0, 12) + '...'
      }
      const mm: MeshMessage = {
        id: i,
        peer_contact_id: -99,
        peer_name: peerName,
        direction: (isSent ? 'sent' : 'received') as 'sent' | 'received',
        plaintext: m.message,
        timestamp: m.timestamp,
        delivered: true,
        encrypted: false,
        message_type: undefined,
        typed_payload: undefined,
        sender_pubkey: null,
        sender_seq: null,
      }
      return mm
    })
  }
  // Hide control envelopes that aren't supposed to show as their own
  // bubbles: reactions (rendered as chips under the target), read receipts
  // (drive the ✓ ticks on outgoing bubbles), edits (mutate the target in
  // place), presence heartbeats, and any other auxiliary metadata. Without
  // this filter every receipt and edit appears as a stray bubble in the
  // chat history. Defense in depth — backend now also drops these on send.
  const HIDDEN_TYPES = new Set(['reaction', 'read_receipt', 'edit', 'presence', 'channel_invite', 'contact_card'])
  const hideReactions = (m: MeshMessage) => !m.message_type || !HIDDEN_TYPES.has(m.message_type)
  if (activeChatChannel.value) {
    return mesh.messages.filter(m => {
      const belongsToChannel = isChannelMessage(m, activeChatChannel.value!.index)
      return belongsToChannel && hideReactions(m)
    })
  }
  if (activeChatPeer.value) {
    // Pull from every underlying contact_id in the merged group so radio
    // and federation-routed messages from the same node land in one thread.
    const merged = activeMergedPeer.value
    const cids = merged ? new Set(merged.contact_ids) : new Set([activeChatPeer.value.contact_id])
    return mesh.messages
      .filter(m => cids.has(m.peer_contact_id) && hideReactions(m))
      .sort((a, b) => Date.parse(a.timestamp) - Date.parse(b.timestamp))
  }
  return []
})

// Fire a read receipt whenever a new received message for the active peer lands.
// Declared after chatMessages so the watch getter doesn't hit a TDZ on registration.
watch(
  () => chatMessages.value.length,
  () => { scheduleReadReceipt() },
)

function isArchyNode(peer: MeshPeer): boolean {
  return peer.advert_name.startsWith('Archy-')
}

/// Build a contact_id → latest-message-timestamp map once per render so
/// we can sort the chat list by recency (freshest thread at the top,
/// Telegram-style). Empty threads keep their lexicographic fallback.
const lastMessageAt = computed<Map<number, number>>(() => {
  const out = new Map<number, number>()
  for (const m of mesh.messages) {
    const ts = Date.parse(m.timestamp)
    if (Number.isNaN(ts)) continue
    const prev = out.get(m.peer_contact_id)
    if (prev === undefined || ts > prev) out.set(m.peer_contact_id, ts)
  }
  return out
})

const sortedPeers = computed(() => {
  return [...mesh.peers].sort((a, b) => {
    const aTs = lastMessageAt.value.get(a.contact_id) ?? 0
    const bTs = lastMessageAt.value.get(b.contact_id) ?? 0
    if (aTs !== bTs) return bTs - aTs
    const aArchy = isArchyNode(a) ? 0 : 1
    const bArchy = isArchyNode(b) ? 0 : 1
    if (aArchy !== bArchy) return aArchy - bArchy
    return a.advert_name.localeCompare(b.advert_name)
  })
})

// Telegram-style unification: a single archipelago node may be discovered
// twice — once as a LoRa contact (firmware-issued mesh pubkey, no DID) and
// again as a synthetic federation peer (archipelago pubkey + DID) created
// the first time it routes a typed message over Tor. Group them into one
// row so the user sees their attachments and radio chat in the same thread.
//
// Merge key resolution, in priority order:
//   1. peer.did (federation-injected peers always carry it)
//   2. federation node whose pubkey matches peer.pubkey_hex
//   3. federation node whose name (case-folded) matches peer.advert_name
//   4. fall back to the peer's own pubkey_hex / advert_name (no merge)
interface MergedPeer {
  key: string
  did: string | null
  display_name: string
  short_did: string | null
  npub: string | null
  contact_ids: number[]
  primary_contact_id: number
  primary_pubkey_hex: string | null
  primary_rssi: number | null
  primary_snr: number | null
  is_archy: boolean
  reachable: boolean
  // The original active-chat marker uses contact_id equality, so keep a
  // representative MeshPeer for the rest of the codepaths that still want
  // a single object (peer header rssi, prekey rotation, etc).
  primary: MeshPeer
}

// Extract the did:key suffix prefix that meshcore embeds in archipelago
// adverts. Meshcore firmware names archipelago nodes "Archy-{first 8 chars
// of the node's did:key z-suffix}", e.g. "Archy-z6Mkn9RY". This lets us
// link a LoRa-discovered radio peer (no DID, no archipelago pubkey) back
// to its federation entry by matching that 8-char prefix against any
// federation node whose DID starts the same way.
function archyAdvertDidPrefix(advertName: string): string | null {
  if (!advertName.startsWith('Archy-')) return null
  const suffix = advertName.slice(6)
  if (suffix.length < 6) return null
  return suffix
}

function fedDidKeySuffix(did: string): string | null {
  // did:key:z6Mk... → z6Mk...
  const idx = did.indexOf(':key:')
  return idx >= 0 ? did.slice(idx + 5) : null
}

function mergeKeyForPeer(peer: MeshPeer): { key: string; matchedFed: FedNodeInfo | null } {
  // Strongest signal: the verified archipelago identity key. The backend binds
  // it onto BOTH twins (federation peer natively, radio twin via
  // `bind_federation_twins`), so grouping by it collapses the cross-transport
  // duplicate regardless of advert name — this is what survives the Meshtastic
  // device rename, which broke the `Archy-z6Mk…` did-prefix match below. Prefer
  // the matching federation node's `did:` key so this stays consistent with the
  // federation-only placeholder pass (which dedups on `did:<did>`); fall back to
  // an `arch:` key only when no federation entry is known for the identity.
  if (peer.arch_pubkey_hex) {
    for (const fed of fedNodesByDid.value.values()) {
      if (fed.pubkey === peer.arch_pubkey_hex) return { key: `did:${fed.did}`, matchedFed: fed }
    }
    return { key: `arch:${peer.arch_pubkey_hex.toLowerCase()}`, matchedFed: null }
  }
  if (peer.did) return { key: `did:${peer.did}`, matchedFed: fedNodesByDid.value.get(peer.did) ?? null }
  // pubkey cross-ref: a federation node may share the archipelago pubkey
  // with this radio peer if it's the same physical node (rare today, since
  // mesh and federation use different ed25519 keys, but kept for robustness)
  if (peer.pubkey_hex) {
    for (const fed of fedNodesByDid.value.values()) {
      if (fed.pubkey === peer.pubkey_hex) return { key: `did:${fed.did}`, matchedFed: fed }
    }
  }
  // did:key prefix cross-ref: meshcore "Archy-z6MkXXXX" → federation
  // node whose did:key suffix starts with the same chars. This is the
  // hot path for archipelago nodes — radio peers carry no DID/pubkey
  // we can use, but the firmware bakes the prefix into the advert.
  const advertPrefix = archyAdvertDidPrefix(peer.advert_name)
  if (advertPrefix) {
    for (const fed of fedNodesByDid.value.values()) {
      const fedSuffix = fedDidKeySuffix(fed.did)
      if (fedSuffix && fedSuffix.startsWith(advertPrefix)) {
        return { key: `did:${fed.did}`, matchedFed: fed }
      }
    }
  }
  // name cross-ref: a federation node whose name (case-folded) matches
  // the LoRa advert name. Last-resort match for non-archipelago meshcore
  // devices configured to advertise their server name verbatim.
  const norm = peer.advert_name.trim().toLowerCase()
  if (norm) {
    for (const fed of fedNodesByDid.value.values()) {
      const fedName = (fed.name ?? '').trim().toLowerCase()
      if (fedName && fedName === norm) return { key: `did:${fed.did}`, matchedFed: fed }
    }
  }
  return { key: `mesh:${peer.pubkey_hex || peer.advert_name || peer.contact_id}`, matchedFed: null }
}

function shortDid(did: string): string {
  // did:archy:<hex64> → did:archy:abcd…wxyz. Other DID methods get truncated
  // generically.
  const idx = did.lastIndexOf(':')
  if (idx > 0 && did.length - idx > 14) {
    const prefix = did.slice(0, idx + 1)
    const id = did.slice(idx + 1)
    return `${prefix}${id.slice(0, 6)}…${id.slice(-4)}`
  }
  return did.length > 24 ? `${did.slice(0, 12)}…${did.slice(-6)}` : did
}

function isSelfRadioPeer(peer: MeshPeer): boolean {
  const selfName = mesh.status?.self_advert_name
  if (selfName && peer.advert_name === selfName) return true
  // Primary fallback: derive the expected advert prefix from our own DID
  // and match it against the radio peer's "Archy-XXXXXXXX" prefix. This
  // works even before mesh.status.self_advert_name is populated and even
  // when there's no federation self-entry to cross-reference.
  const advertPrefix = archyAdvertDidPrefix(peer.advert_name)
  if (advertPrefix && selfDid.value) {
    const ourSuffix = fedDidKeySuffix(selfDid.value)
    if (ourSuffix?.startsWith(advertPrefix)) return true
  }
  return false
}

const mergedPeers = computed<MergedPeer[]>(() => {
  const groups = new Map<string, MergedPeer>()
  for (const peer of sortedPeers.value) {
    if (isSelfRadioPeer(peer)) continue
    const { key, matchedFed } = mergeKeyForPeer(peer)
    const existing = groups.get(key)
    if (existing) {
      existing.contact_ids.push(peer.contact_id)
      // Prefer a federation-enriched display name even if the second peer
      // is the radio one — keeps the row stable across discovery order.
      if (matchedFed?.name && existing.display_name === existing.primary.advert_name) {
        existing.display_name = matchedFed.name
      }
      if (matchedFed?.npub && !existing.npub) existing.npub = matchedFed.npub
      if (matchedFed?.did && !existing.did) {
        existing.did = matchedFed.did
        existing.short_did = shortDid(matchedFed.did)
      }
    } else {
      const did = peer.did ?? matchedFed?.did ?? null
      const stub = { did, primary_pubkey_hex: peer.pubkey_hex }
      const alias = aliasFor(stub)
      groups.set(key, {
        key,
        did,
        display_name: alias || matchedFed?.name || peer.advert_name || `Node #${peer.contact_id}`,
        short_did: did ? shortDid(did) : null,
        npub: matchedFed?.npub ?? null,
        contact_ids: [peer.contact_id],
        primary_contact_id: peer.contact_id,
        primary_pubkey_hex: peer.pubkey_hex,
        primary_rssi: peer.rssi,
        primary_snr: peer.snr,
        is_archy: isArchyNode(peer) || !!matchedFed,
        reachable: peer.reachable ?? true,
        primary: peer,
      })
    }
  }
  // Surface every federation node as its own row even if no radio peer has
  // been matched against it yet. The user always sees the canonical server
  // names ("Arch Dev", "ArchISO") in the list, and clicking one starts a
  // chat that routes over Tor until a radio link is also discovered, at
  // which point the rows transparently merge under the same DID key.
  for (const fed of fedNodesByDid.value.values()) {
    // Skip our own federation self-entry — every node accidentally has one
    // because the federation list isn't filtered server-side. Match by
    // onion instead of DID so it works even when names are missing.
    if (selfTorOnion.value) {
      const ours = selfTorOnion.value.replace(/\.onion$/, '')
      const theirs = fed.onion.replace(/\.onion$/, '')
      if (ours === theirs) continue
    }
    const key = `did:${fed.did}`
    if (groups.has(key)) continue
    // Synthesise a placeholder MeshPeer so openChat() and the existing
    // rssi/avatar template paths don't need a separate code path for
    // "federation-only" rows. The contact_id MUST match the backend's
    // federation-synthetic id (high bit set, derived from the pubkey) so
    // `mesh.send` accepts it (it parses contact_id as u64 — a negative
    // placeholder fails with "Missing contact_id") and so federation-routed
    // messages stored under that id land in this thread.
    const synthCid = federationContactId(fed.pubkey)
    const placeholder: MeshPeer = {
      contact_id: synthCid,
      advert_name: fed.name || fed.did,
      pubkey_hex: fed.pubkey,
      did: fed.did,
      rssi: null,
      hops: null,
      last_heard: null,
    } as unknown as MeshPeer
    const alias = aliasFor({ did: fed.did, primary_pubkey_hex: fed.pubkey })
    groups.set(key, {
      key,
      did: fed.did,
      display_name: alias || fed.name || shortDid(fed.did),
      short_did: shortDid(fed.did),
      npub: fed.npub,
      contact_ids: [synthCid],
      primary_contact_id: synthCid,
      primary_pubkey_hex: fed.pubkey,
      primary_rssi: null,
      primary_snr: null,
      is_archy: true,
      reachable: true,
      primary: placeholder,
    })
  }
  return Array.from(groups.values())
})

// Contact search — filters the Peers list by name, DID, npub, or pubkey.
const peerSearch = ref('')
const displayedPeers = computed<MergedPeer[]>(() => {
  const q = peerSearch.value.trim().toLowerCase()
  if (!q) return mergedPeers.value
  return mergedPeers.value.filter((mp) =>
    mp.display_name.toLowerCase().includes(q) ||
    (mp.short_did?.toLowerCase().includes(q) ?? false) ||
    (mp.did?.toLowerCase().includes(q) ?? false) ||
    (mp.npub?.toLowerCase().includes(q) ?? false) ||
    (mp.primary_pubkey_hex?.toLowerCase().includes(q) ?? false),
  )
})

// Mirror of the backend's `federation_peer_contact_id` (mesh/mod.rs): take the
// first 4 bytes of the archipelago pubkey as a little-endian u32, clear the top
// bit, then set it as the federation marker. Producing the SAME id here means a
// federation-only row addresses the exact contact the backend routes/stores
// under, instead of an unsendable negative placeholder.
function federationContactId(pubkeyHex: string): number {
  const bytes = pubkeyHex.match(/../g)?.map(h => parseInt(h, 16)) ?? []
  if (bytes.length < 4) return 0x80000001
  const [b0, b1, b2, b3] = bytes as [number, number, number, number]
  const low = (b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)) >>> 0
  return ((0x80000000 | (low & 0x7fffffff)) >>> 0)
}

// activeChatPeer is a single MeshPeer (the row the user clicked). To unify
// the chat we need the merged group it belongs to so chatMessages can pull
// from every underlying contact_id, not just the one that was clicked.
const activeMergedPeer = computed<MergedPeer | null>(() => {
  const peer = activeChatPeer.value
  if (!peer) return null
  return mergedPeers.value.find(mp => mp.contact_ids.includes(peer.contact_id)) ?? null
})

function mergedUnreadCount(mp: MergedPeer): number {
  let total = 0
  for (const cid of mp.contact_ids) total += mesh.unreadCounts[cid] || 0
  return total
}

// Live contact_id -> friendly display name, rebuilt from the current peer
// table. Received messages snapshot a `peer_name` at receive time, so a text
// that arrived before its sender's NodeInfo is stuck showing a synthetic
// "Meshtastic !xxxx" id forever. Resolving the name live here means the bubble
// always shows the current archy identity (e.g. "Arch Optiplex") the moment it
// is learned — in every view, without rewriting stored rows.
const nameByContactId = computed<Map<number, string>>(() => {
  const out = new Map<number, string>()
  for (const mp of mergedPeers.value) {
    for (const cid of mp.contact_ids) out.set(cid, mp.display_name)
  }
  return out
})

// A generic, non-identifying placeholder the backend stamps before the real
// sender is known. We never want these to win over a live archy name.
function isPlaceholderName(name: string | null | undefined): boolean {
  if (!name) return true
  return /^Meshtastic !?[0-9a-f]{1,8}$/i.test(name)
    || /^Channel \d+$/.test(name)
    || /^Node #/.test(name)
    || name === 'dm-via-channel'
    || name === 'Unknown'
}

// Sender label shown above a RECEIVED bubble. Only meaningful in multi-sender
// views (mesh channels + the Archipelago channel) where the header alone can't
// attribute each message; 1:1 DM threads already name the peer in the header.
// Prefers the live archy identity over the snapshotted name.
function senderLabelFor(msg: MeshMessage): string | null {
  if (msg.direction !== 'received') return null
  if (!activeChatChannel.value && !archChannelActive.value) return null
  const live = nameByContactId.value.get(msg.peer_contact_id)
  if (live && !isPlaceholderName(live)) return live
  // The sender name snapshotted on the row. For a stock public-channel device a
  // synthetic "Meshtastic !xxxx" id IS the best identity we have, so keep it —
  // only suppress the genuinely contentless group/placeholder labels.
  const snap = msg.peer_name
  if (snap && !/^Channel \d+$/.test(snap) && snap !== 'dm-via-channel' && snap !== 'Unknown') {
    return snap
  }
  return live || 'Unknown sender'
}

// Inline contact rename in the chat header. The pencil button toggles an
// input bound to renameDraft; commit fires mesh.contacts-save keyed by
// DID (or pubkey hex as fallback) so the alias propagates everywhere
// `aliasFor` is consulted in the merged peer list.
const renamingActive = ref(false)
const renameDraft = ref('')
const renameInputEl = ref<HTMLInputElement | null>(null)
function startRename() {
  const mp = activeMergedPeer.value
  if (!mp) return
  renameDraft.value = mp.display_name
  renamingActive.value = true
  nextTick(() => renameInputEl.value?.focus())
}
function cancelRename() {
  renamingActive.value = false
  renameDraft.value = ''
}
async function commitRename() {
  if (!renamingActive.value) return
  const mp = activeMergedPeer.value
  if (!mp) { cancelRename(); return }
  const next = renameDraft.value.trim()
  renamingActive.value = false
  // Empty string → clear the alias and fall back to derived name.
  const key = mp.did || mp.primary_pubkey_hex
  if (!key) return
  // Optimistic local update so the header changes immediately.
  if (next) contactAliases.value.set(key, next)
  else contactAliases.value.delete(key)
  try {
    await rpcClient.meshContactsSave(key, next || null)
    await refreshContacts()
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'Rename failed'
  }
}

function openChat(peer: MeshPeer) {
  activeChatPeer.value = peer
  activeChatChannel.value = null
  archChannelActive.value = false
  sendError.value = ''
  messageText.value = ''
  activeTab.value = 'chat'
  mobileShowChat.value = true
  nextTick(() => {
    // Clear unread only after the chat has rendered and auto-scrolled to the
    // latest messages ("you've actually seen them"), and clear ALL twins of
    // the merged conversation — the badge sums across every contact_id, so
    // clearing just the clicked row's id left the badge stuck.
    scrollChatToBottom()
    mesh.markChatRead(activeMergedPeer.value?.contact_ids ?? peer.contact_id)
  })
}

function openChannelChat(channel: { index: number; name: string }) {
  activeChatChannel.value = channel
  activeChatPeer.value = null
  archChannelActive.value = false
  sendError.value = ''
  messageText.value = ''
  activeTab.value = 'chat'
  mobileShowChat.value = true
  const contactIds = channel.index === publicChannel.index
    ? mesh.messages.filter(isPublicChannelMessage).map(message => message.peer_contact_id)
    : [channelContactId(channel.index)]
  mesh.markChatRead([...new Set([channelContactId(channel.index), ...contactIds])])
  nextTick(() => scrollChatToBottom())
}

function closeChat() {
  activeChatPeer.value = null
  activeChatChannel.value = null
  archChannelActive.value = false
  if (archPollInterval) { clearInterval(archPollInterval); archPollInterval = null }
  mobileShowChat.value = false
  mesh.clearViewingChat()
}

async function handleSendMessage() {
  // Single-flight guard: the input's `@keydown.enter` fires per keydown, so a
  // repeating/held Enter or a rapid Enter→click before the button's disabled
  // state flips queues a second mesh.send against the same text. That's how
  // every bubble was showing up twice — sender transmitted the same envelope
  // twice, receiver stored both. Bail if a send is already in flight.
  if (mesh.sending || sendingArch.value) return
  if (archChannelActive.value) {
    await sendArchMessage()
    nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    return
  }
  // Pending reply: Send flushes as mesh.send-reply targeting the stashed
  // MessageKey. Takes precedence over a pending attachment — we don't try
  // to express "attach-as-reply" in one go.
  // Pending edit: Send flushes as mesh.edit-message against the stashed seq.
  if (pendingEdit.value && activeChatPeer.value) {
    if (!messageText.value.trim()) return
    sendError.value = ''
    try {
      await mesh.editMessage(
        activeChatPeer.value.contact_id,
        pendingEdit.value.target_seq,
        messageText.value.trim(),
      )
      messageText.value = ''
      pendingEdit.value = null
      nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    } catch (err: unknown) {
      sendError.value = err instanceof Error ? err.message : 'Edit failed'
    }
    return
  }
  if (pendingReply.value && activeChatPeer.value) {
    if (!messageText.value.trim()) return
    sendError.value = ''
    try {
      await mesh.sendReply(
        activeChatPeer.value.contact_id,
        pendingReply.value.target_pubkey,
        pendingReply.value.target_seq,
        messageText.value.trim(),
      )
      messageText.value = ''
      pendingReply.value = null
      nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    } catch (err: unknown) {
      sendError.value = err instanceof Error ? err.message : 'Reply failed'
    }
    return
  }
  // Pending share-to-mesh attachment: Send flushes the CID as a ContentRef
  // rather than a plain text message. Any text in the input becomes the
  // caption. Only valid for direct peers (channel broadcast of content_ref
  // isn't in scope for Phase 3c).
  if (pendingAttachment.value && activeChatPeer.value) {
    sendError.value = ''
    try {
      const caption = messageText.value.trim() || undefined
      await mesh.sendContent(activeChatPeer.value.contact_id, pendingAttachment.value.cid, caption)
      messageText.value = ''
      pendingAttachment.value = null
      nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    } catch (err: unknown) {
      sendError.value = err instanceof Error ? err.message : 'Share failed'
    }
    return
  }
  if (!messageText.value.trim()) return
  sendError.value = ''
  try {
    if (activeChatChannel.value) {
      await mesh.sendChannelMessage(activeChatChannel.value.index, messageText.value)
      messageText.value = ''
      nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    } else if (activeChatPeer.value) {
      await mesh.sendMessage(activeChatPeer.value.contact_id, messageText.value)
      messageText.value = ''
      nextTick(() => { scrollChatToBottom(); refocusMessageInput() })
    }
  } catch (err: unknown) {
    sendError.value = err instanceof Error ? err.message : 'Send failed'
  }
}

function scrollChatToBottom() {
  if (chatScrollEl.value) {
    chatScrollEl.value.scrollTop = chatScrollEl.value.scrollHeight
    mesh.viewingAtBottom = true
  }
}

// Scroll-position read tracking: while the latest messages are in view the
// open conversation stays "read" (new arrivals don't badge); scrolled up into
// history, incoming messages accumulate unread like any normal chat app —
// scrolling back down clears them.
function onChatScroll() {
  const el = chatScrollEl.value
  if (!el) return
  const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48
  mesh.viewingAtBottom = nearBottom
  if (nearBottom && activeMergedPeer.value) {
    mesh.markChatRead(activeMergedPeer.value.contact_ids)
  }
  scheduleReadReceipt()
}

// Keep the compose field focused after a send so the user can keep typing
// without re-clicking it (Enter re-focuses natively, but the Send button
// click otherwise leaves focus on the button).
function refocusMessageInput() {
  messageInputRef.value?.focus()
}

// Wheel over the chat must scroll ONLY the chat — never leak to the contacts
// list or the page. Bound with `@wheel.stop.prevent`: `.stop` keeps the event
// from reaching the global controller-nav wheel handler (which would otherwise
// also scroll whatever container is focused, e.g. the peer list after a click),
// and `.prevent` stops the native page scroll. We then apply the delta to the
// chat container directly.
function onChatWheel(e: WheelEvent) {
  const el = chatScrollEl.value
  if (!el) return
  el.scrollTop += e.deltaY
}

async function handleBroadcast() {
  broadcasting.value = true
  broadcastResult.value = null
  try {
    await mesh.broadcastIdentity()
    broadcastResult.value = 'ok'
  } catch (e) {
    broadcastResult.value = e instanceof Error ? e.message : 'Broadcast failed'
  } finally {
    broadcasting.value = false
    setTimeout(() => { broadcastResult.value = null }, 4000)
  }
}

async function handleRefresh() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    // Backend first: re-query the radio's contact table (mesh.refresh), then
    // re-read EVERYTHING the list is built from — peers, contacts/aliases,
    // federation nodes, outbox — not just the mesh caches.
    await Promise.allSettled([
      mesh.refreshRadio(),
      mesh.refreshAll(),
      refreshContacts(),
      refreshFederationNodes(),
      refreshOutboxCount(),
    ])
    // Radio contact refresh is async on the backend — pick up its result.
    await mesh.fetchPeers()
  } finally {
    refreshing.value = false
  }
}

async function handleToggleEnabled() {
  configuring.value = true
  try {
    const newEnabled = !(mesh.status?.enabled ?? false)
    await mesh.configure({ enabled: newEnabled })
  } finally { configuring.value = false }
}

async function handleConnectDevice(devicePath: string) {
  connectingDevice.value = devicePath
  try {
    await mesh.configure({ enabled: true, device_path: devicePath })
  } finally {
    connectingDevice.value = null
  }
}

function signalBars(rssi: number | null, snr: number | null = null): number {
  if (rssi !== null) {
    if (rssi > -60) return 4
    if (rssi > -75) return 3
    if (rssi > -90) return 2
    return 1
  }
  // Meshcore carries no per-packet RSSI, only SNR — fall back to it so the
  // bars aren't permanently empty for that transport (Reticulum has neither,
  // stays at 0/"No signal data yet", which is honest — RNS/LXMF genuinely
  // doesn't expose a signal-quality metric).
  if (snr !== null) {
    if (snr > 5) return 4
    if (snr > 0) return 3
    if (snr > -10) return 2
    return 1
  }
  return 0
}

function signalTitle(rssi: number | null, snr: number | null): string {
  if (rssi === null && snr === null) return 'No signal data yet'
  const parts: string[] = []
  if (rssi !== null) parts.push(`${rssi} dBm`)
  if (snr !== null) parts.push(`SNR ${snr.toFixed(1)} dB`)
  return parts.join(' · ')
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const secs = Math.floor(diff / 1000)
  if (secs < 60) return `${secs}s ago`
  const mins = Math.floor(secs / 60)
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  return `${hours}h ago`
}

function truncatePubkey(hex: string | null): string {
  if (!hex) return ''
  return hex.slice(0, 8) + '...' + hex.slice(-6)
}

// ── Reply + Reaction (Phase 2a) ───────────────────────────────────────────
// Pending reply state: when the user picks "Reply" on a bubble, we stash its
// MessageKey here; next Send uses mesh.send-reply instead of mesh.send.
interface PendingReply {
  target_pubkey: string
  target_seq: number
  preview: string
}
const pendingReply = ref<PendingReply | null>(null)
const actionMenuForId = ref<number | null>(null)

function openActionMenu(msgId: number, ev?: Event) {
  ev?.stopPropagation()
  actionMenuForId.value = actionMenuForId.value === msgId ? null : msgId
  reactionPickerForId.value = null
}
function closeActionMenu() {
  actionMenuForId.value = null
  reactionPickerForId.value = null
}
function handleDocClickForMenu(ev: MouseEvent) {
  if (actionMenuForId.value === null) return
  const target = ev.target as HTMLElement | null
  if (!target) return
  if (target.closest('.mesh-chat-action-menu')) return
  if (target.closest('.mesh-chat-action-trigger')) return
  actionMenuForId.value = null
}
function messageKeyFor(msg: { sender_pubkey?: string | null; sender_seq?: number | null }): { pubkey: string; seq: number } | null {
  if (!msg.sender_pubkey || msg.sender_seq == null) return null
  return { pubkey: msg.sender_pubkey, seq: msg.sender_seq }
}
function startReplyTo(msg: MeshMessage) {
  const key = messageKeyFor(msg)
  if (!key) return
  pendingReply.value = {
    target_pubkey: key.pubkey,
    target_seq: key.seq,
    preview: summarizeForPreview(msg),
  }
  closeActionMenu()
}
function clearPendingReply() {
  pendingReply.value = null
}

// ── Edit / Delete / Forward (Phase 2b) ───────────────────────────────────
// Pending edit: when the user picks "Edit" on an own message, we stash its
// sender_seq and prefill the composer. Next Send calls mesh.edit-message
// instead of mesh.send.
interface PendingEdit { target_seq: number; original_text: string }
const pendingEdit = ref<PendingEdit | null>(null)
function startEditOf(msg: MeshMessage) {
  if (msg.direction !== 'sent' || msg.sender_seq == null) return
  pendingEdit.value = { target_seq: msg.sender_seq, original_text: msg.plaintext }
  messageText.value = msg.plaintext
  closeActionMenu()
}
function clearPendingEdit() {
  pendingEdit.value = null
  messageText.value = ''
}
async function deleteOwnMessage(msg: MeshMessage) {
  if (msg.direction !== 'sent' || msg.sender_seq == null || !activeChatPeer.value) return
  const ok = await appConfirm({
    title: 'Delete message',
    message: 'Delete this message? Peers already received it — this only marks it as deleted.',
    confirmLabel: 'Delete',
    danger: true,
  })
  if (!ok) return
  try {
    await mesh.deleteMessage(activeChatPeer.value.contact_id, msg.sender_seq)
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'delete failed'
  } finally {
    closeActionMenu()
  }
}
async function forwardToCurrent(msg: MeshMessage) {
  if (!activeChatPeer.value) return
  try {
    await mesh.forwardMessage(activeChatPeer.value.contact_id, msg.id)
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'forward failed'
  } finally {
    closeActionMenu()
  }
}
function isEditedMessage(msg: MeshMessage): number | null {
  const ts = msg.typed_payload?.edited_at
  return typeof ts === 'number' ? ts : null
}
function isDeletedMessage(msg: MeshMessage): boolean {
  return msg.message_type === 'delete' || msg.typed_payload?.deleted === true
}
/// Short label for the per-message transport pill (Meshtastic / Meshcore /
/// Reticulum / FIPS / Tor), or null when the transport isn't known.
function transportLabel(msg: MeshMessage): string | null {
  switch (msg.transport) {
    case 'meshtastic': return 'Meshtastic'
    case 'meshcore': return 'Meshcore'
    case 'reticulum': return 'Reticulum'
    case 'lora': return 'LoRa'
    case 'fips': return 'FIPS'
    case 'tor': return 'Tor'
    default: return null
  }
}

// Read-receipt: after render, if the bottom message is from the peer (direction='received')
// and has a MessageKey, fire mesh.send-read-receipt up to that seq. Debounced so scroll
// doesn't spam the wire.
const lastReceiptSentForSeq = ref<Map<number, number>>(new Map()) // contactId → last acked seq
let receiptDebounce: ReturnType<typeof setTimeout> | null = null
function scheduleReadReceipt() {
  if (receiptDebounce) clearTimeout(receiptDebounce)
  receiptDebounce = setTimeout(() => {
    const peer = activeChatPeer.value
    if (!peer) return
    const received = chatMessages.value.filter((m) => m.direction === 'received' && m.sender_seq != null)
    if (received.length === 0) return
    const latest = received[received.length - 1]
    if (!latest) return
    const latestSeq = latest.sender_seq as number
    const already = lastReceiptSentForSeq.value.get(peer.contact_id) ?? -1
    if (latestSeq <= already) return
    const pubkey = latest.sender_pubkey
    if (!pubkey) return
    lastReceiptSentForSeq.value.set(peer.contact_id, latestSeq)
    void mesh.sendReadReceipt(peer.contact_id, pubkey, latestSeq)
  }, 400)
}

// Hop visualization modal — click a message's transport pill to see how it
// traveled: radio hops + signal quality for LoRa transports, overlay/circuit
// shape for FIPS/Tor. Signal numbers are the PEER's current values (per-link,
// not stored per-message) — labelled as such in the modal.
const hopVizMsg = ref<MeshMessage | null>(null)
const hopVizPeer = computed<MeshPeer | null>(() => {
  const m = hopVizMsg.value
  if (!m) return null
  return mesh.peers.find(p => p.contact_id === m.peer_contact_id) ?? activeChatPeer.value
})
function signalQualityLabel(snr: number | null, rssi: number | null): string {
  if (snr == null && rssi == null) return 'signal unknown'
  if (snr != null) {
    if (snr > 5) return 'excellent signal'
    if (snr > 0) return 'good signal'
    if (snr > -10) return 'fair signal'
    return 'weak signal'
  }
  if (rssi != null) {
    if (rssi > -80) return 'excellent signal'
    if (rssi > -100) return 'good signal'
    if (rssi > -115) return 'fair signal'
    return 'weak signal'
  }
  return 'signal unknown'
}

// Reaction dropdown: the quick-reaction row lives behind a "React" toggle so
// the action menu stays compact and a larger emoji palette fits.
const reactionPickerForId = ref<number | null>(null)
const REACTION_PALETTE = ['👍', '❤️', '😂', '😮', '😢', '🙏', '🔥', '🎉', '💯', '👀', '😡', '🫡']

const reactionInFlight = ref<string | null>(null) // `${msgId}:${emoji}` while RPC is running
async function reactTo(msg: MeshMessage, emoji: string) {
  const key = messageKeyFor(msg)
  if (!key || !activeChatPeer.value) return
  const marker = `${msg.id}:${emoji}`
  reactionInFlight.value = marker
  try {
    await mesh.sendReaction(activeChatPeer.value.contact_id, key.pubkey, key.seq, emoji)
    closeActionMenu()
  } catch (e) {
    sendError.value = e instanceof Error ? e.message : 'reaction failed'
  } finally {
    if (reactionInFlight.value === marker) reactionInFlight.value = null
  }
}

/// Build a map from target MessageKey (pubkey:seq) → emoji strings seen in
/// the current message window. Iterates in timestamp order so the freshest
/// reaction per (reactor, target) wins; an empty emoji clears the reactor.
interface ReactionChip { emoji: string; count: number; by_self: boolean }
const reactionIndex = computed<Map<string, ReactionChip[]>>(() => {
  const perTarget = new Map<string, Map<string, string>>() // target → (reactor → emoji)
  for (const m of mesh.messages) {
    if (m.message_type !== 'reaction' || !m.typed_payload) continue
    const target = m.typed_payload.target as { sender_pubkey?: string; sender_seq?: number } | undefined
    if (!target?.sender_pubkey || target.sender_seq == null) continue
    const key = `${target.sender_pubkey}:${target.sender_seq}`
    const reactor = m.direction === 'sent' ? '__self__' : (m.sender_pubkey ?? `peer:${m.peer_contact_id}`)
    let slot = perTarget.get(key)
    if (!slot) { slot = new Map(); perTarget.set(key, slot) }
    const emoji = String(m.typed_payload.emoji ?? '')
    if (emoji === '') {
      slot.delete(reactor)
    } else {
      slot.set(reactor, emoji)
    }
  }
  const out = new Map<string, ReactionChip[]>()
  for (const [key, slot] of perTarget) {
    const counts = new Map<string, { count: number; by_self: boolean }>()
    for (const [reactor, emoji] of slot) {
      const cur = counts.get(emoji) ?? { count: 0, by_self: false }
      cur.count++
      if (reactor === '__self__') cur.by_self = true
      counts.set(emoji, cur)
    }
    out.set(key, Array.from(counts, ([emoji, v]) => ({ emoji, count: v.count, by_self: v.by_self })))
  }
  return out
})
function reactionsFor(msg: { sender_pubkey?: string | null; sender_seq?: number | null }): ReactionChip[] {
  const key = messageKeyFor(msg)
  if (!key) return []
  return reactionIndex.value.get(`${key.pubkey}:${key.seq}`) ?? []
}
/// Lookup the target of a reply bubble so we can show a mini-quote above it.
function replyTargetPreview(msg: MeshMessage): string | null {
  if (msg.message_type !== 'reply' || !msg.typed_payload) return null
  const target = msg.typed_payload.target as { sender_pubkey?: string; sender_seq?: number } | undefined
  if (!target?.sender_pubkey || target.sender_seq == null) return null
  const match = mesh.messages.find(
    (m) => m.sender_pubkey === target.sender_pubkey && m.sender_seq === target.sender_seq,
  )
  if (!match) return `→ ${String(target.sender_pubkey).slice(0, 8)}…#${target.sender_seq}`
  return summarizeForPreview(match)
}
function summarizeForPreview(m: MeshMessage): string {
  const text = m.plaintext?.trim()
  if (text) return text.slice(0, 80)
  switch (m.message_type) {
    case 'content_ref': return `📎 ${m.typed_payload?.filename || m.typed_payload?.mime || 'attachment'}`
    case 'invoice': return `⚡ ${(m.typed_payload?.amount_sats || 0).toLocaleString()} sats`
    case 'coordinate': return '📍 Location'
    case 'alert': return `🚨 ${m.typed_payload?.message || 'Alert'}`.slice(0, 80)
    default: return `(${m.message_type || 'message'})`
  }
}

// ── share-to-mesh iframe intent (Phase 3c) ────────────────────────────────
// Marketplace app iframes POST a file to `/api/share-to-mesh` then call
// `window.parent.postMessage({type:'share-to-mesh', cid, ...})`. We park the
// CID as a pending attachment; next time the user picks a peer and hits Send
// (with optional caption text), we call mesh.send-content on that CID.
interface PendingAttachment {
  cid: string
  size: number
  mime: string
  filename: string | null
  self_url?: string
}
const pendingAttachment = ref<PendingAttachment | null>(null)

function loadPendingFromSession() {
  try {
    const raw = sessionStorage.getItem('archipelago_share_to_mesh')
    if (!raw) return
    sessionStorage.removeItem('archipelago_share_to_mesh')
    const data = JSON.parse(raw) as { cid?: string; size?: number; mime?: string; filename?: string | null; self_url?: string }
    if (!data.cid) return
    pendingAttachment.value = {
      cid: data.cid,
      size: data.size ?? 0,
      mime: data.mime ?? 'application/octet-stream',
      filename: data.filename ?? null,
      self_url: data.self_url,
    }
  } catch {
    /* ignore */
  }
}

function clearPendingAttachment() {
  pendingAttachment.value = null
}

// ── ContentRef attach + fetch (Phase 3b) ──────────────────────────────────
const attaching = ref(false)
// Compose row's "+" attach menu — a single toggle button that pops open a
// small vertical stack (attach file / voice message) instead of cramming
// each option in as its own always-visible button (was overflowing).
const showAttachMenu = ref(false)
function closeAttachMenuOnOutsideClick(ev: MouseEvent) {
  if (!showAttachMenu.value) return
  const target = ev.target as HTMLElement
  if (!target.closest('.mesh-attach-menu-anchor')) showAttachMenu.value = false
}
// Listener lifecycle handled by the combined onActivated/onDeactivated block
// above (T-02-15: this used to be its own onMounted/onUnmounted pair, moved
// here so it stops firing while Mesh is off screen instead of leaking for
// the rest of the session once Mesh is kept alive).
const attachError = ref<string | null>(null)
const fetchingCids = ref<Set<string>>(new Set())
const fetchedUrls = ref<Map<string, string>>(new Map())

// Auto-render attachments whose bytes are already local — an inline
// (mesh.send-content-inline) ContentRef has its bytes written to our
// BlobStore the moment it's sent/received (dispatch.rs), so there's no real
// fetch to wait on; skip the explicit "Download" click for those. Runs
// `immediate: true` so already-loaded history gets the same treatment as
// newly-arriving messages.
const autoFetchedCids = new Set<string>()

// NOT `immediate: true`. This watcher calls handleFetchContent, whose body
// touches consts declared further down the setup block — and an immediate
// watcher runs DURING setup, before those exist. Vue surfaced it as
// "ReferenceError: Cannot access 'b' before initialization" from
// Ye.immediate, and it fired whenever history already contained an inline
// content_ref, taking the whole Mesh view down with it.
//
// onMounted runs after setup completes, so every binding is initialized and
// already-loaded history still gets the same treatment as new messages —
// which is what `immediate` was there for.
function autoFetchInlineContent() {
    for (const msg of chatMessages.value) {
      const payload = msg.typed_payload as { cid?: string; inline?: boolean } | undefined
      if (
        msg.message_type === 'content_ref' &&
        payload?.inline &&
        payload.cid &&
        !fetchedUrls.value.has(payload.cid) &&
        !fetchingCids.value.has(payload.cid) &&
        !autoFetchedCids.has(payload.cid)
      ) {
        autoFetchedCids.add(payload.cid)
        void handleFetchContent(msg.typed_payload as any)
      }
    }
}

watch(() => chatMessages.value.length, autoFetchInlineContent)
onMounted(autoFetchInlineContent)

// Transport chooser modal state — populated when advice comes back as
// "choose" (size fits both inline-over-mesh AND Tor). User picks a path;
// `transportChoiceResolve` finishes the promise started by handleAttachFile.
interface PendingTransportChoice {
  file: File
  size: number
  est_seconds: number
  has_tor: boolean
}
const transportChoice = ref<PendingTransportChoice | null>(null)
let transportChoiceResolve: ((choice: 'mesh' | 'tor' | 'cancel') => void) | null = null

function pickTransport(choice: 'mesh' | 'tor' | 'cancel') {
  if (transportChoiceResolve) {
    transportChoiceResolve(choice)
    transportChoiceResolve = null
  }
  transportChoice.value = null
}

// Image quality-picker modal — shown before sending an image attachment.
// Presets skip 'original'-specific compression (handled in compressImage)
// but still show a transfer-time estimate for it, fetched the same way as
// every other preset via the existing mesh.transport-advice RPC.
const imageQualityChoice = ref<{ file: File } | null>(null)
const imageQualityEstimates = ref<Map<string, string>>(new Map())
let imageQualityResolve: ((preset: ImageCompressionPreset | null) => void) | null = null
// Transport pills at the bottom of the image modal: when the peer is also a
// federation node (FIPS overlay and/or Tor), the user can send the image that
// way instead of over LoRa. 'lora' keeps the existing radio flow; 'fips'/'tor'
// route via the blob/content-ref path (the dial layer prefers FIPS with Tor
// fallback — the pills are capability labels, defaulted to whichever of the
// two actually worked last for this peer).
const imageSendTransport = ref<'lora' | 'fips' | 'tor'>('lora')
const imageTransportOptions = ref<{ fips: boolean; tor: boolean }>({ fips: false, tor: false })

function formatEstSeconds(seconds: number): string {
  if (seconds < 60) return `~${seconds}s`
  return `~${Math.round(seconds / 60)}m`
}

async function openImageQualityDialog(file: File, peerContactId: number): Promise<ImageCompressionPreset | null> {
  imageQualityChoice.value = { file }
  imageQualityEstimates.value = new Map()
  imageSendTransport.value = 'lora'
  imageTransportOptions.value = { fips: false, tor: false }
  // Fire off estimates for all presets in parallel — each preset's nominal
  // target size (or the real file size for 'original') against the SAME
  // mesh.transport-advice RPC the non-image attach flow already uses.
  void Promise.all(
    IMAGE_COMPRESSION_PRESETS.map(async (preset) => {
      const size = preset.key === 'original' ? file.size : Math.min(preset.targetBytes, file.size)
      try {
        const advice = await mesh.transportAdvice(peerContactId, size)
        const label =
          advice.tier === 'impossible' ? 'too large' : formatEstSeconds(advice.est_seconds)
        imageQualityEstimates.value = new Map(imageQualityEstimates.value).set(preset.key, label)
        imageTransportOptions.value = { fips: advice.has_fips, tor: advice.has_tor }
      } catch {
        imageQualityEstimates.value = new Map(imageQualityEstimates.value).set(preset.key, '?')
      }
    }),
  )
  return new Promise<ImageCompressionPreset | null>((resolve) => {
    imageQualityResolve = resolve
  })
}

function pickImageQuality(preset: ImageCompressionPreset | null) {
  if (imageQualityResolve) {
    imageQualityResolve(preset)
    imageQualityResolve = null
  }
  imageQualityChoice.value = null
}

async function resolveFederationOnion(peerName: string): Promise<string | undefined> {
  try {
    const fed = await rpcClient.federationListNodes()
    const hit = fed.nodes.find(
      (n: { name?: string; onion?: string }) =>
        (n.name ?? '').toLowerCase() === peerName.toLowerCase() ||
        (n.name ?? '').toLowerCase().includes(peerName.toLowerCase()) ||
        peerName.toLowerCase().includes((n.name ?? '').toLowerCase()),
    )
    return hit?.onion ?? undefined
  } catch {
    return undefined
  }
}

async function sendViaMeshInline(file: File, peerContactId: number) {
  const buf = await file.arrayBuffer()
  const bytes = new Uint8Array(buf)
  await mesh.sendContentInline(
    peerContactId,
    file.type || 'application/octet-stream',
    bytes,
    file.name,
    messageText.value.trim() || undefined,
  )
}

async function sendViaTorContentRef(file: File, peerContactId: number, peerName: string) {
  const buf = await file.arrayBuffer()
  const headers: Record<string, string> = {
    'X-Blob-Mime': file.type || 'application/octet-stream',
    'X-Blob-Filename': file.name,
    'Content-Type': 'application/octet-stream',
  }
  // Tiny thumbnail so the receiver sees a preview immediately instead of
  // waiting on an explicit fetch — see the content_ref render branch below.
  if (file.type.startsWith('image/')) {
    try {
      const thumb = await makeThumbnail(file)
      headers['X-Blob-Thumb'] = btoa(String.fromCharCode(...thumb))
    } catch {
      // Best-effort — a missing thumbnail just means no preview, not a failed send.
    }
  }
  const up = await fetch('/api/blob', {
    method: 'POST',
    headers,
    credentials: 'include',
    body: buf,
  })
  if (!up.ok) throw new Error(`upload failed: ${up.status}`)
  const { cid } = (await up.json()) as { cid: string }
  const peerOnion = await resolveFederationOnion(peerName)
  await mesh.sendContent(peerContactId, cid, messageText.value.trim() || undefined, peerOnion)
}

/** Resolve the best transport for `file` via mesh.transport-advice (prompting
 * with the transport-chooser modal for the ambiguous "choose" tier) and send
 * it. Shared by the file-attach flow and voice messages — both just need
 * "given a File and a peer, get it there". Returns false if the user
 * cancelled or the send was rejected as impossible (caller already informed
 * via attachError); true on success. */
async function sendFileViaBestTransport(file: File, peer: MeshPeer): Promise<boolean> {
  const advice = await mesh.transportAdvice(peer.contact_id, file.size)
  let transport: 'mesh' | 'tor' | 'cancel'
  if (advice.tier === 'auto-mesh' || advice.tier === 'resource-mesh') {
    // 'resource-mesh' (Reticulum-only, large-over-LoRa via RNS Resource) is
    // routed by the SAME mesh.send-content-inline call as 'auto-mesh' — the
    // backend decides internally whether to use the small inline-chunk path
    // or a Resource transfer based on size + active device type.
    transport = 'mesh'
  } else if (advice.tier === 'tor-only') {
    transport = 'tor'
  } else if (advice.tier === 'impossible') {
    attachError.value = `Cannot send: ${advice.reason} (${(file.size / 1024).toFixed(1)} KB)`
    return false
  } else {
    // "choose" — open modal and wait for user to pick
    transport = await new Promise<'mesh' | 'tor' | 'cancel'>((resolve) => {
      transportChoiceResolve = resolve
      transportChoice.value = {
        file,
        size: file.size,
        est_seconds: advice.est_seconds,
        has_tor: advice.has_tor,
      }
    })
    if (transport === 'cancel') return false
  }
  if (transport === 'mesh') {
    await sendViaMeshInline(file, peer.contact_id)
  } else {
    await sendViaTorContentRef(file, peer.contact_id, peer.advert_name)
  }
  return true
}

async function handleAttachFile(ev: Event) {
  const input = ev.target as HTMLInputElement
  let file = input.files?.[0]
  if (!file) return
  if (!activeChatPeer.value) {
    attachError.value = 'Pick a peer first'
    if (input) input.value = ''
    return
  }
  const peer = activeChatPeer.value
  attaching.value = true
  attachError.value = null
  try {
    let forcedTransport: 'fips' | 'tor' | null = null
    if (file.type.startsWith('image/')) {
      const preset = await openImageQualityDialog(file, peer.contact_id)
      if (!preset) return // user cancelled
      if (imageSendTransport.value !== 'lora') forcedTransport = imageSendTransport.value
      file = await compressImage(file, preset)
    }
    if (forcedTransport) {
      // User picked a FIPS/Tor pill in the image modal — skip the LoRa
      // advice/chooser flow entirely and ship via the content-ref path.
      await sendViaTorContentRef(file, peer.contact_id, peer.advert_name)
    } else if (!(await sendFileViaBestTransport(file, peer))) return
    messageText.value = ''
    nextTick(() => scrollChatToBottom())
  } catch (e) {
    attachError.value = e instanceof Error ? e.message : 'attach failed'
  } finally {
    attaching.value = false
    if (input) input.value = ''
  }
}

// Voice messages — async/store-and-forward (a recorded clip sent as a normal
// attachment), NOT a live call; reuses sendFileViaBestTransport exactly like
// any other file. Hold-to-record: press the mic button, release to send.
const isRecordingVoice = ref(false)
let voiceRecorder: MediaRecorder | null = null
let voiceRecorderStream: MediaStream | null = null
let voiceChunks: Blob[] = []

async function startVoiceRecording() {
  if (isRecordingVoice.value || attaching.value || !activeChatPeer.value) return
  // navigator.mediaDevices is only exposed by the browser in a secure context
  // (HTTPS, or localhost) — undefined entirely on plain-HTTP LAN nodes like
  // .116, which throws a confusing "Cannot read properties of undefined"
  // instead of the permission-style error handled below.
  if (!navigator.mediaDevices?.getUserMedia) {
    attachError.value = 'Voice messages need HTTPS (or localhost) — the browser blocks microphone access on plain HTTP'
    return
  }
  try {
    voiceRecorderStream = await navigator.mediaDevices.getUserMedia({ audio: true })
  } catch (e) {
    attachError.value = e instanceof Error ? e.message : 'Microphone access denied'
    return
  }
  voiceChunks = []
  voiceRecorder = new MediaRecorder(voiceRecorderStream, { mimeType: 'audio/webm;codecs=opus' })
  voiceRecorder.ondataavailable = (e) => {
    if (e.data.size > 0) voiceChunks.push(e.data)
  }
  voiceRecorder.start()
  isRecordingVoice.value = true
}

async function stopVoiceRecording() {
  if (!isRecordingVoice.value || !voiceRecorder) return
  const recorder = voiceRecorder
  const stream = voiceRecorderStream
  isRecordingVoice.value = false
  voiceRecorder = null
  voiceRecorderStream = null
  const blob = await new Promise<Blob>((resolve) => {
    recorder.onstop = () => resolve(new Blob(voiceChunks, { type: 'audio/webm' }))
    recorder.stop()
  })
  stream?.getTracks().forEach((t) => t.stop())
  if (blob.size === 0 || !activeChatPeer.value) return

  const peer = activeChatPeer.value
  const file = new File([blob], `voice-${Date.now()}.webm`, { type: 'audio/webm' })
  attaching.value = true
  attachError.value = null
  try {
    if (await sendFileViaBestTransport(file, peer)) {
      nextTick(() => scrollChatToBottom())
    }
  } catch (e) {
    attachError.value = e instanceof Error ? e.message : 'Failed to send voice message'
  } finally {
    attaching.value = false
  }
}

/** pointerleave while still holding (e.g. dragged off the button) — stop and
 * send rather than silently discarding the in-progress recording. */
function stopVoiceRecordingIfActive() {
  if (isRecordingVoice.value) void stopVoiceRecording()
}

async function handleFetchContent(payload: {
  cid: string
  sender_onion: string
  cap_token: string
  cap_exp: number
  mime?: string
  filename?: string | null
}) {
  if (fetchingCids.value.has(payload.cid)) return
  fetchingCids.value.add(payload.cid)
  try {
    const res = await mesh.fetchContent({
      cid: payload.cid,
      sender_onion: payload.sender_onion,
      cap_token: payload.cap_token,
      cap_exp: payload.cap_exp,
      mime: payload.mime,
      filename: payload.filename ?? undefined,
    })
    const r = res as { local_url?: string }
    if (r.local_url) {
      fetchedUrls.value.set(payload.cid, r.local_url)
      fetchedUrls.value = new Map(fetchedUrls.value)
    }
  } catch (e) {
    console.error('fetch-content failed', e)
  } finally {
    fetchingCids.value.delete(payload.cid)
  }
}

function isImageMime(mime?: string): boolean {
  return !!mime && mime.startsWith('image/')
}

const IMAGE_MIME_TO_EXT: Record<string, string> = {
  'image/jpeg': 'jpg',
  'image/png': 'png',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'image/bmp': 'bmp',
  'image/svg+xml': 'svg',
}

// Lightbox is FileBrowserItem-shaped (built for the cloud file browser) — a
// mesh attachment isn't a real file-browser entry, so we key it by `cid`
// and hand back the already-fetched blob URL instead of a real fetch.
const lightboxOpen = ref(false)
const lightboxItems = ref<FileBrowserItem[]>([])

type MeshAttachmentPayload = {
  cid: string
  sender_onion: string
  cap_token: string
  cap_exp: number
  mime?: string
  filename?: string | null
}

async function openMeshLightbox(payload: MeshAttachmentPayload) {
  if (!fetchedUrls.value.has(payload.cid)) {
    await handleFetchContent(payload)
  }
  if (!fetchedUrls.value.has(payload.cid)) return // fetch failed
  const ext = IMAGE_MIME_TO_EXT[payload.mime || ''] || 'jpg'
  lightboxItems.value = [
    {
      name: payload.filename || `image.${ext}`,
      path: payload.cid,
      size: 0,
      modified: '',
      isDir: false,
      type: payload.mime || 'image/jpeg',
      extension: ext,
    },
  ]
  lightboxOpen.value = true
}

async function lightboxFetchBlobUrl(cid: string): Promise<string> {
  return fetchedUrls.value.get(cid) || ''
}

async function downloadAttachment(payload: MeshAttachmentPayload) {
  if (!fetchedUrls.value.has(payload.cid)) {
    await handleFetchContent(payload)
  }
  const url = fetchedUrls.value.get(payload.cid)
  if (!url) return
  const a = document.createElement('a')
  a.href = url
  a.download = payload.filename || `image.${IMAGE_MIME_TO_EXT[payload.mime || ''] || 'jpg'}`
  document.body.appendChild(a)
  a.click()
  a.remove()
}

</script>

<template>
  <div class="mesh-view">
    <!-- Header (hidden on mobile) -->
    <div class="mesh-header hidden md:flex">
      <div class="mesh-header-left">
        <div class="mesh-title-row">
          <h1 class="mesh-title">Mesh Network</h1>
          <RefreshIndicator :state="meshRefreshIndicatorState" label="Refreshing mesh status" />
        </div>
        <p class="mesh-subtitle">
          {{ mesh.status?.peer_count ?? 0 }} peer{{ (mesh.status?.peer_count ?? 0) !== 1 ? 's' : '' }}
          <span v-if="mesh.status?.device_connected" class="mesh-subtitle-badge">Live</span>
          <span v-else-if="mesh.status?.device_present" class="mesh-subtitle-badge">Mesh device detected</span>
        </p>
      </div>
      <button
        class="glass-button mesh-flasher-btn"
        :disabled="!flashLoraPath"
        :title="flashLoraPath ? '' : 'Plug in a LoRa radio first'"
        @click="openFlashLora"
      >
        Flash LoRa
      </button>
    </div>

    <!-- Error banner -->
    <div v-if="mesh.error" class="mesh-error">{{ mesh.error }}</div>

    <!-- Responsive column layout -->
    <div class="mesh-columns" :class="{ 'mesh-columns-wide': isWideDesktop, 'mesh-columns-very-wide': isVeryWideDesktop }">
      <!-- LEFT COLUMN: Status + Peers -->
      <div class="mesh-left" data-controller-zone="mesh-left" :class="{ 'mobile-hidden': mobileShowChat || mobileTab !== 'chat' }">
        <!-- Device Status -->
        <div data-controller-container tabindex="0" class="glass-card mesh-status-card" :class="{ 'mesh-status-collapsed': !deviceExpanded }">
          <div class="mesh-status-header" role="button" tabindex="0" @click="deviceExpanded = !deviceExpanded" @keydown.enter.prevent="deviceExpanded = !deviceExpanded" @keydown.space.prevent="deviceExpanded = !deviceExpanded">
            <div class="mesh-status-indicator" :class="mesh.status?.device_connected ? 'connected' : 'disconnected'" />
            <h2 class="mesh-section-title">Device</h2>
            <svg class="mesh-status-chevron" :aria-expanded="deviceExpanded" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
            </svg>
          </div>

          <div v-if="mesh.loading && !mesh.status" class="mesh-loading">Loading...</div>

          <div v-else-if="mesh.status" class="mesh-status-grid">
            <div class="mesh-stat">
              <span class="mesh-stat-label">Status</span>
              <span class="mesh-stat-value" :class="mesh.status.device_connected ? 'text-green' : mesh.status.device_present ? 'text-orange' : mesh.status.enabled ? 'text-orange' : 'text-muted'">
                {{ mesh.status.device_connected ? 'Broadcasting' : mesh.status.device_present ? 'Device detected — connecting' : mesh.status.enabled ? 'Connecting...' : 'Disabled' }}
              </span>
            </div>
            <div class="mesh-stat">
              <span class="mesh-stat-label">Type</span>
              <span class="mesh-stat-value">{{ mesh.status.device_type === 'unknown' ? '—' : mesh.status.device_type }}</span>
            </div>
            <div class="mesh-stat">
              <span class="mesh-stat-label">Port</span>
              <span class="mesh-stat-value">{{ mesh.status.device_path ?? 'Auto' }}</span>
            </div>
            <div class="mesh-stat">
              <span class="mesh-stat-label">Sent</span>
              <span class="mesh-stat-value">{{ mesh.status.messages_sent }}</span>
            </div>
            <div class="mesh-stat">
              <span class="mesh-stat-label">Recv</span>
              <span class="mesh-stat-value">{{ mesh.status.messages_received }}</span>
            </div>
            <div class="mesh-stat">
              <span class="mesh-stat-label">Channel</span>
              <span class="mesh-stat-value">{{ mesh.status.channel_name }}</span>
            </div>
          </div>

          <!-- Detected USB devices -->
          <div v-if="mesh.status?.detected_devices?.length" class="mesh-detected-devices">
            <div v-for="dev in mesh.status.detected_devices" :key="dev" class="mesh-device-row">
              <div class="mesh-device-indicator" />
              <span class="mesh-device-path">{{ dev }}</span>
              <button
                v-if="!mesh.status?.device_connected"
                class="glass-button mesh-connect-btn"
                :disabled="connectingDevice !== null"
                @click="handleConnectDevice(dev)"
              >
                {{ connectingDevice === dev ? 'Connecting...' : 'Connect' }}
              </button>
            </div>
          </div>
        </div>

        <!-- Off-grid mode banner -->
        <div v-if="transport.meshOnly" class="mesh-offgrid-banner">
          <svg class="w-4 h-4 text-orange-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18.364 5.636a9 9 0 11-12.728 0M12 9v4m0 4h.01" />
          </svg>
          <span class="text-sm font-medium text-orange-300">OFF-GRID</span>
          <span class="text-xs text-white/50">Tor/FIPS disabled - LoRa only</span>
        </div>

        <!-- Actions row -->
        <div class="mesh-actions" data-controller-container tabindex="0">
          <button class="glass-button mesh-action-btn" :disabled="configuring" @click="handleToggleEnabled">
            {{ mesh.status?.enabled ? 'Disable' : 'Enable' }}
          </button>
          <button
            class="glass-button mesh-action-btn"
            :class="broadcastResult === 'ok' ? 'mesh-action-ok' : ''"
            :disabled="!mesh.status?.device_connected || broadcasting"
            :title="broadcastResult && broadcastResult !== 'ok' ? broadcastResult : 'Announce this node so nearby radios learn about it'"
            @click="handleBroadcast"
          >
            {{ broadcasting ? 'Sending…' : broadcastResult === 'ok' ? 'Sent ✓' : broadcastResult ? 'Failed ✕' : 'Broadcast' }}
          </button>
          <button
            class="glass-button mesh-action-btn"
            :class="transport.meshOnly ? 'mesh-offgrid-active' : ''"
            :disabled="togglingOffGrid"
            @click="handleToggleOffGrid"
          >
            {{ transport.meshOnly ? 'Go Online' : 'Off-Grid' }}
          </button>
          <!-- While refreshing show ONLY the spinner — spinner + "Refreshing…"
               together overflow the button's fixed width. -->
          <button class="glass-button mesh-action-btn" :disabled="refreshing" @click="handleRefresh" :title="refreshing ? 'Refreshing…' : 'Re-query the radio and reload peers, contacts and federation nodes'">
            <span v-if="refreshing" class="mesh-refresh-spinner" aria-hidden="true"></span>
            <template v-if="!refreshing">Refresh</template>
          </button>
        </div>

        <!-- Peers list -->
        <div data-controller-container tabindex="0" class="glass-card mesh-peers-card">
          <div class="flex items-center justify-between">
            <h2 class="mesh-section-title">Peers <span class="mesh-peer-count">{{ mesh.peers.length }}</span></h2>
            <button class="text-xs text-white/40 hover:text-red-400 transition-colors px-2 py-1" @click="clearAllMesh" title="Clear all peers, messages, and chat history">Clear All</button>
          </div>

          <!-- Contact search: filters the list below by name / DID / npub / pubkey -->
          <div class="mesh-peer-search-wrap">
            <input
              v-model="peerSearch"
              type="text"
              class="mesh-peer-search"
              placeholder="Search contacts…"
              aria-label="Search contacts"
              data-controller-no-submit
            />
            <button
              v-if="peerSearch"
              type="button"
              class="mesh-peer-search-clear"
              aria-label="Clear search"
              title="Clear search"
              @click="peerSearch = ''"
            >&times;</button>
          </div>

          <!-- Only claim "no peers" when the MERGED list (radio + federation)
               is truly empty — with no radio attached the federation rows and
               the two channel rows must still render. -->
          <div v-if="displayedPeers.length === 0 && !mesh.status?.device_connected" class="mesh-empty">
            No peers discovered yet.
          </div>

          <div v-else class="mesh-peer-list">
            <!-- Archipelago Channel -->
            <div
              class="mesh-peer-row is-channel"
              :class="{ active: archChannelActive }"
              tabindex="0"
              role="button"
              @click="openArchChannel"
              @keydown.enter="openArchChannel"
            >
              <div class="mesh-peer-avatar channel" style="background: rgba(251,146,60,0.2); color: #fb923c;">A</div>
              <div class="mesh-peer-info">
                <div class="mesh-peer-name">Archipelago</div>
                <div class="mesh-peer-sub">All nodes over Tor</div>
              </div>
              <span v-if="archUnread > 0" class="ml-auto text-[10px] px-1.5 py-0.5 rounded-full bg-orange-500/30 text-orange-300">{{ archUnread }}</span>
            </div>
            <!-- Public channel -->
            <div
              class="mesh-peer-row is-channel"
              :class="{ active: activeChatChannel?.index === 0 }"
              tabindex="0"
              role="button"
              @click="openChannelChat(publicChannel)"
              @keydown.enter="openChannelChat(publicChannel)"
            >
              <div class="mesh-peer-avatar channel">#</div>
              <div class="mesh-peer-info">
                <div class="mesh-peer-name">Public</div>
                <div class="mesh-peer-sub">Mesh radio</div>
              </div>
              <span v-if="publicChannelUnread" class="ml-auto text-[10px] px-1.5 py-0.5 rounded-full bg-orange-500/30 text-orange-300">{{ publicChannelUnread }}</span>
            </div>
            <div v-if="displayedPeers.length === 0 && peerSearch.trim()" class="mesh-empty">
              No contacts match “{{ peerSearch.trim() }}”.
            </div>
            <div
              v-for="mp in displayedPeers" :key="mp.key"
              class="mesh-peer-row"
              :class="{ active: mp.contact_ids.includes(activeChatPeer?.contact_id ?? -1), 'is-archy': mp.is_archy }"
              tabindex="0"
              role="button"
              @click="openChat(mp.primary)"
              @keydown.enter="openChat(mp.primary)"
            >
              <div class="mesh-peer-avatar" :class="{ archy: mp.is_archy }">
                <AnimatedLogo v-if="mp.is_archy" size="sm" />
                <template v-else>{{ mp.display_name.charAt(0).toUpperCase() }}</template>
                <span class="mesh-peer-reach" :class="mp.reachable ? 'is-reachable' : 'is-unreachable'" :title="mp.reachable ? 'Reachable' : 'Not currently reachable'"></span>
              </div>
              <div class="mesh-peer-info">
                <div class="mesh-peer-name">
                  {{ mp.display_name }}
                  <span v-if="mp.is_archy" class="mesh-peer-archy-badge">Archy</span>
                </div>
                <div class="mesh-peer-sub">
                  <template v-if="mp.short_did">{{ mp.short_did }}</template>
                  <template v-else>{{ truncatePubkey(mp.primary_pubkey_hex) }}</template>
                </div>
                <div v-if="mp.npub" class="mesh-peer-sub mesh-peer-npub">{{ mp.npub.slice(0, 12) }}…{{ mp.npub.slice(-6) }}</div>
              </div>
              <span v-if="mergedUnreadCount(mp)" class="mesh-unread-badge">
                {{ mergedUnreadCount(mp) }}
              </span>
              <div class="mesh-peer-signal" :title="signalTitle(mp.primary_rssi, mp.primary_snr)">
                <div class="mesh-signal-bars">
                  <div v-for="i in 4" :key="i" class="mesh-signal-bar" :class="{ active: i <= signalBars(mp.primary_rssi, mp.primary_snr) }" />
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- RIGHT COLUMN: Tabbed panels -->
      <div class="mesh-right" data-controller-zone="mesh-chat" :class="{ 'mobile-hidden': !mobileShowChat || mobileTab !== 'chat' }">
        <!-- Tab bar (medium desktop only) -->
        <div v-if="showTabBar" class="mesh-tab-bar">
          <button class="mesh-tab" :class="{ active: activeTab === 'chat' }" @click="activeTab = 'chat'">Chat</button>
          <button class="mesh-tab" :class="{ active: activeTab === 'bitcoin' }" @click="activeTab = 'bitcoin'">Bitcoin</button>
          <button class="mesh-tab" :class="{ active: activeTab === 'deadman' }" @click="activeTab = 'deadman'">
            Dead Man
            <span v-if="mesh.deadmanStatus?.triggered" class="mesh-tab-badge mesh-tab-badge-alert">!</span>
          </button>
          <button class="mesh-tab" :class="{ active: activeTab === 'assistant' }" @click="activeTab = 'assistant'">
            AI
          </button>
          <button class="mesh-tab" :class="{ active: activeTab === 'map' }" @click="activeTab = 'map'">
            Map
          </button>
          <button class="mesh-tab" :class="{ active: activeTab === 'device' }" @click="activeTab = 'device'" title="Device settings">
            📡
          </button>
        </div>

        <!-- Chat Panel -->
        <div v-if="showChatPanel" data-controller-container tabindex="0" class="glass-card mesh-chat-card" :class="{ 'mesh-chat-card-active': hasActiveChat }">
          <div v-if="!hasActiveChat" class="mesh-chat-empty">
            <div class="mesh-chat-empty-icon">&#x1F4E1;</div>
            <p>Select a peer or channel to chat</p>
            <p class="mesh-chat-empty-sub">Messages are sent over LoRa mesh radio</p>
          </div>
          <template v-else>
            <!-- Mobile: floating back button (shared glass pill style), pinned
                 above the tab bar — replaces the in-header arrow so the back
                 control is no longer crammed inside the chat container. -->
            <Teleport v-if="isViewActive" to="body">
              <button
                type="button"
                class="mesh-chat-mobile-back mobile-back-btn back-button-glass px-6 py-3 rounded-xl font-medium items-center justify-center gap-2"
                @click="closeChat"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
                <span>Back</span>
              </button>
            </Teleport>
            <div class="mesh-chat-header">
              <div class="mesh-chat-header-info">
                <div class="mesh-chat-header-name">
                  <template v-if="renamingActive">
                    <input
                      ref="renameInputEl"
                      v-model="renameDraft"
                      class="mesh-chat-header-rename-input"
                      :placeholder="activeChatName"
                      @keyup.enter="commitRename"
                      @keyup.esc="cancelRename"
                      @blur="commitRename"
                    />
                  </template>
                  <template v-else>
                    {{ activeChatName }}
                    <button
                      v-if="activeMergedPeer"
                      class="mesh-chat-header-rename"
                      title="Rename this contact"
                      @click="startRename"
                    >✎</button>
                  </template>
                  <span v-if="activeChatPeer && isArchyNode(activeChatPeer)" class="mesh-peer-archy-badge">Archy</span>
                  <span v-if="activeChatChannel" class="mesh-peer-channel-badge">Channel</span>
                </div>
                <div class="mesh-chat-header-sub">{{ activeChatSub }}</div>
              </div>
              <div class="mesh-chat-header-status">
                <span v-if="activeChatPeer && peerSessionInfo" class="mesh-session-badge" :class="peerSessionInfo.forward_secrecy ? 'session-ratchet' : peerSessionInfo.has_session ? 'session-static' : 'session-none'" :title="peerSessionInfo.forward_secrecy ? 'Double Ratchet (forward secrecy)' : peerSessionInfo.has_session ? 'Static encryption' : 'No encryption'">&#x1F6E1;</span>
                <button v-if="activeChatPeer && peerSessionInfo" class="mesh-session-rotate" :disabled="rotatingPrekeys" :title="'Rotate prekeys'" @click="handleRotatePrekeys">{{ rotatingPrekeys ? '…' : '⟲' }}</button>
                <span v-if="outboxCount > 0" class="mesh-outbox-badge" :title="outboxCount + ' queued messages waiting for delivery'">📤 {{ outboxCount }}</span>
                <span v-if="activeChatPeer" class="mesh-chat-header-time">{{ timeAgo(activeChatPeer.last_heard) }}</span>
              </div>
            </div>
            <div ref="chatScrollEl" class="mesh-chat-messages" @scroll="onChatScroll" @wheel.stop.prevent="onChatWheel">
              <div v-if="chatMessages.length === 0" class="mesh-chat-no-messages">
                No messages yet. Say hello!
              </div>
              <div
                v-for="msg in chatMessages" :key="msg.id"
                class="mesh-chat-bubble-wrapper"
                :class="msg.direction"
              >
                <div class="mesh-chat-bubble" :class="[msg.direction, msg.message_type ? 'typed-' + msg.message_type : '', { 'menu-open': actionMenuForId === msg.id }]">
                  <div v-if="senderLabelFor(msg)" class="mesh-chat-bubble-sender">{{ senderLabelFor(msg) }}</div>
                  <div v-if="replyTargetPreview(msg)" class="mesh-chat-reply-quote">
                    ↳ {{ replyTargetPreview(msg) }}
                  </div>
                  <!-- Invoice card -->
                  <div v-if="msg.message_type === 'invoice' && msg.typed_payload" class="mesh-typed-invoice">
                    <div class="mesh-typed-invoice-header">
                      <span class="mesh-typed-icon">&#x26A1;</span>
                      <span class="mesh-typed-label">Lightning Invoice</span>
                      <span v-if="msg.typed_payload.paid" class="mesh-typed-paid">Paid</span>
                    </div>
                    <div class="mesh-typed-invoice-amount">{{ (msg.typed_payload.amount_sats || 0).toLocaleString() }} sats</div>
                    <div v-if="msg.typed_payload.memo" class="mesh-typed-invoice-memo">{{ msg.typed_payload.memo }}</div>
                    <div class="mesh-typed-invoice-bolt11">{{ (msg.typed_payload.bolt11 || '').substring(0, 40) }}...</div>
                  </div>
                  <!-- Alert card -->
                  <div v-else-if="msg.message_type === 'alert' && msg.typed_payload" class="mesh-typed-alert" :class="'alert-' + (msg.typed_payload.alert_type || 'status')">
                    <div class="mesh-typed-alert-header">
                      <span class="mesh-typed-icon">{{ msg.typed_payload.alert_type === 'emergency' ? '&#x1F6A8;' : msg.typed_payload.alert_type === 'dead_man' ? '&#x2620;' : '&#x2139;' }}</span>
                      <span class="mesh-typed-label">{{ msg.typed_payload.alert_type === 'emergency' ? 'EMERGENCY' : msg.typed_payload.alert_type === 'dead_man' ? 'DEAD MAN' : 'Status' }}</span>
                      <span v-if="msg.typed_payload.signed" class="mesh-typed-signed">Signed</span>
                    </div>
                    <div class="mesh-typed-alert-message">{{ msg.typed_payload.message }}</div>
                    <a v-if="msg.typed_payload.coordinate" class="mesh-typed-alert-location" :href="'https://www.openstreetmap.org/?mlat=' + (msg.typed_payload.coordinate.lat / 1000000) + '&mlon=' + (msg.typed_payload.coordinate.lng / 1000000) + '&zoom=14'" target="_blank" rel="noopener">
                      &#x1F4CD; {{ msg.typed_payload.coordinate.label || 'View location' }}
                    </a>
                  </div>
                  <!-- Coordinate card -->
                  <div v-else-if="msg.message_type === 'coordinate' && msg.typed_payload" class="mesh-typed-coordinate">
                    <div class="mesh-typed-coordinate-header">
                      <span class="mesh-typed-icon">&#x1F4CD;</span>
                      <span class="mesh-typed-label">Location</span>
                    </div>
                    <div class="mesh-typed-coordinate-value">{{ (msg.typed_payload.lat / 1000000).toFixed(4) }}, {{ (msg.typed_payload.lng / 1000000).toFixed(4) }}</div>
                    <div v-if="msg.typed_payload.label" class="mesh-typed-coordinate-label">{{ msg.typed_payload.label }}</div>
                    <a class="mesh-typed-coordinate-link" :href="'https://www.openstreetmap.org/?mlat=' + (msg.typed_payload.lat / 1000000) + '&mlon=' + (msg.typed_payload.lng / 1000000) + '&zoom=14'" target="_blank" rel="noopener">Open Map</a>
                  </div>
                  <!-- Block header -->
                  <div v-else-if="msg.message_type === 'block_header' && msg.typed_payload" class="mesh-typed-block">
                    <span class="mesh-typed-icon">&#x26D3;</span>
                    <span class="mesh-typed-label">{{ msg.typed_payload.message || msg.plaintext }}</span>
                  </div>
                  <!-- TX relay request -->
                  <div v-else-if="msg.message_type === 'tx_relay' && msg.typed_payload" class="mesh-typed-block">
                    <span class="mesh-typed-icon">&#x21AA;</span>
                    <span class="mesh-typed-label">TX relay #{{ msg.typed_payload.request_id }} ({{ (msg.typed_payload.tx_hex || '').length }} hex chars)</span>
                  </div>
                  <!-- TX relay response -->
                  <div v-else-if="msg.message_type === 'tx_relay_response' && msg.typed_payload" class="mesh-typed-block">
                    <span class="mesh-typed-icon">{{ msg.typed_payload.txid ? '&#x2705;' : '&#x274C;' }}</span>
                    <span class="mesh-typed-label">
                      <template v-if="msg.typed_payload.txid">Broadcast #{{ msg.typed_payload.request_id }}: {{ String(msg.typed_payload.txid).substring(0, 12) }}…</template>
                      <template v-else>TX relay failed #{{ msg.typed_payload.request_id }}: {{ msg.typed_payload.error || 'unknown' }}</template>
                    </span>
                  </div>
                  <!-- TX confirmation -->
                  <div v-else-if="msg.message_type === 'tx_confirmation' && msg.typed_payload" class="mesh-typed-block">
                    <span class="mesh-typed-icon">&#x26D3;</span>
                    <span class="mesh-typed-label">{{ msg.typed_payload.confirmations }} conf @ block {{ msg.typed_payload.block_height }} — {{ String(msg.typed_payload.txid || '').substring(0, 12) }}…</span>
                  </div>
                  <!-- Lightning relay request -->
                  <div v-else-if="msg.message_type === 'lightning_relay' && msg.typed_payload" class="mesh-typed-invoice">
                    <div class="mesh-typed-invoice-header">
                      <span class="mesh-typed-icon">&#x26A1;</span>
                      <span class="mesh-typed-label">Lightning Relay Request</span>
                    </div>
                    <div class="mesh-typed-invoice-amount">{{ (msg.typed_payload.amount_sats || 0).toLocaleString() }} sats</div>
                    <div class="mesh-typed-invoice-bolt11">{{ (msg.typed_payload.bolt11 || '').substring(0, 40) }}…</div>
                  </div>
                  <!-- Lightning relay response -->
                  <div v-else-if="msg.message_type === 'lightning_relay_response' && msg.typed_payload" class="mesh-typed-invoice">
                    <div class="mesh-typed-invoice-header">
                      <span class="mesh-typed-icon">{{ msg.typed_payload.preimage ? '&#x2705;' : '&#x274C;' }}</span>
                      <span class="mesh-typed-label">
                        <template v-if="msg.typed_payload.preimage">Lightning Paid</template>
                        <template v-else>Lightning Failed</template>
                      </span>
                    </div>
                    <div v-if="msg.typed_payload.payment_hash" class="mesh-typed-invoice-bolt11">hash: {{ String(msg.typed_payload.payment_hash).substring(0, 20) }}…</div>
                    <div v-if="msg.typed_payload.preimage" class="mesh-typed-invoice-bolt11">preimage: {{ String(msg.typed_payload.preimage).substring(0, 20) }}…</div>
                    <div v-if="msg.typed_payload.error" class="mesh-typed-invoice-memo">{{ msg.typed_payload.error }}</div>
                  </div>
                  <div v-else-if="msg.message_type === 'content_ref' && msg.typed_payload" class="mesh-typed-content">
                    <div class="mesh-typed-content-meta">
                      <span class="mesh-typed-icon">📎</span>
                      <span class="mesh-typed-label">{{ msg.typed_payload.filename || msg.typed_payload.mime }}</span>
                      <span class="mesh-typed-content-size">{{ msg.typed_payload.size }} B</span>
                    </div>
                    <div v-if="msg.typed_payload.caption" class="mesh-typed-content-caption">{{ msg.typed_payload.caption }}</div>
                    <template v-if="fetchedUrls.get(msg.typed_payload.cid)">
                      <div v-if="isImageMime(msg.typed_payload.mime)" class="mesh-typed-content-image-wrap">
                        <img
                          :src="fetchedUrls.get(msg.typed_payload.cid)"
                          class="mesh-typed-content-preview"
                          alt="attachment"
                          role="button"
                          title="Click to view full-size"
                          @click="openMeshLightbox(msg.typed_payload as any)"
                        />
                        <button
                          class="mesh-typed-content-download-btn"
                          title="Download"
                          aria-label="Download image"
                          @click="downloadAttachment(msg.typed_payload as any)"
                        >
                          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                            <path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5" />
                            <path d="M5 19h14" />
                          </svg>
                        </button>
                      </div>
                      <audio
                        v-else-if="(msg.typed_payload.mime || '').startsWith('audio/')"
                        :src="fetchedUrls.get(msg.typed_payload.cid)"
                        controls
                        class="mesh-typed-content-audio"
                      />
                      <a v-else :href="fetchedUrls.get(msg.typed_payload.cid)" target="_blank" class="btn">Open</a>
                    </template>
                    <template v-else-if="msg.direction === 'sent'">
                      <span class="mesh-typed-content-hint">(shared from this node)</span>
                    </template>
                    <template v-else>
                      <img
                        v-if="msg.typed_payload.thumb_bytes && isImageMime(msg.typed_payload.mime)"
                        :src="`data:${msg.typed_payload.mime};base64,${msg.typed_payload.thumb_bytes}`"
                        class="mesh-typed-content-preview mesh-typed-content-thumb"
                        alt="thumbnail preview"
                        role="button"
                        title="Click to view full-size"
                        @click="openMeshLightbox(msg.typed_payload as any)"
                      />
                      <button
                        class="mesh-typed-content-fetch-btn"
                        :disabled="fetchingCids.has(msg.typed_payload.cid)"
                        @click="handleFetchContent(msg.typed_payload as any)"
                      >
                        <span
                          v-if="fetchingCids.has(msg.typed_payload.cid)"
                          class="mesh-refresh-spinner"
                          aria-hidden="true"
                        ></span>
                        <svg v-else viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                          <path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5" />
                          <path d="M5 19h14" />
                        </svg>
                        {{ fetchingCids.has(msg.typed_payload.cid) ? 'Fetching…' : 'Download' }}
                      </button>
                    </template>
                  </div>
                  <!-- Forwarded message -->
                  <div v-else-if="msg.message_type === 'forward' && msg.typed_payload" class="mesh-chat-bubble-text">
                    <div class="mesh-chat-forward-header">↪ Forwarded from {{ msg.typed_payload.orig_name || 'unknown' }}</div>
                    <div class="mesh-chat-forward-body">{{ msg.plaintext }}</div>
                  </div>
                  <!-- Deleted tombstone -->
                  <div v-else-if="isDeletedMessage(msg)" class="mesh-chat-bubble-text mesh-chat-deleted">🗑 message deleted</div>
                  <!-- Default: plain text -->
                  <div v-else class="mesh-chat-bubble-text">{{ msg.plaintext }}</div>
                  <div class="mesh-chat-bubble-meta">
                    <span v-if="transportLabel(msg)" class="mesh-chat-transport mesh-chat-transport-clickable" :class="'transport-' + msg.transport" :title="'Delivered over ' + transportLabel(msg) + ' — click for route details'" @click.stop="hopVizMsg = msg">{{ transportLabel(msg) }}</span>
                    <button class="mesh-chat-more-btn" title="How this message traveled" aria-label="Show message route" @click.stop="hopVizMsg = msg">⋯</button>
                    <span v-if="msg.encrypted" class="mesh-chat-e2e">E2E</span>
                    <span v-if="isEditedMessage(msg) !== null" class="mesh-chat-edited">(edited)</span>
                    <span v-if="msg.delivered && msg.direction === 'sent'" class="mesh-chat-ack">&#x2713;&#x2713;</span>
                    <span class="mesh-chat-bubble-time">{{ timeAgo(msg.timestamp) }}</span>
                    <button
                      v-if="messageKeyFor(msg) && msg.message_type !== 'reaction'"
                      class="mesh-chat-action-trigger"
                      :class="{ active: actionMenuForId === msg.id }"
                      :title="actionMenuForId === msg.id ? 'Close' : 'React / Reply'"
                      @click.stop="openActionMenu(msg.id, $event)"
                    >&#x22EF;</button>
                  </div>
                  <div v-if="reactionsFor(msg).length > 0" class="mesh-chat-reactions">
                    <span
                      v-for="chip in reactionsFor(msg)"
                      :key="chip.emoji"
                      class="mesh-chat-reaction-chip"
                      :class="{ 'by-self': chip.by_self }"
                    >{{ chip.emoji }}<span v-if="chip.count > 1" class="mesh-chat-reaction-count">{{ chip.count }}</span></span>
                  </div>
                  <div
                    v-if="actionMenuForId === msg.id && messageKeyFor(msg) && msg.message_type !== 'reaction' && activeChatPeer"
                    class="mesh-chat-action-menu"
                    @click.stop
                  >
                    <button class="mesh-chat-action-btn" :disabled="reactionInFlight !== null" @click="startReplyTo(msg)">Reply</button>
                    <button class="mesh-chat-action-btn" :disabled="reactionInFlight !== null" @click="forwardToCurrent(msg)">Forward</button>
                    <button v-if="msg.direction === 'sent'" class="mesh-chat-action-btn" :disabled="reactionInFlight !== null" @click="startEditOf(msg)">Edit</button>
                    <button v-if="msg.direction === 'sent'" class="mesh-chat-action-btn mesh-chat-action-danger" :disabled="reactionInFlight !== null" @click="deleteOwnMessage(msg)">Delete</button>
                    <button
                      class="mesh-chat-action-btn"
                      :class="{ active: reactionPickerForId === msg.id }"
                      :disabled="reactionInFlight !== null"
                      @click="reactionPickerForId = reactionPickerForId === msg.id ? null : msg.id"
                    >😀 React ▾</button>
                    <button class="mesh-chat-action-btn" :disabled="reactionInFlight !== null" @click="closeActionMenu">✕</button>
                    <div v-if="reactionPickerForId === msg.id" class="mesh-chat-reaction-dropdown">
                      <button
                        v-for="emoji in REACTION_PALETTE"
                        :key="emoji"
                        class="mesh-chat-reaction-btn"
                        :class="{ 'is-busy': reactionInFlight === `${msg.id}:${emoji}` }"
                        :disabled="reactionInFlight !== null"
                        @click="reactTo(msg, emoji)"
                      >
                        <span v-if="reactionInFlight === `${msg.id}:${emoji}`" class="mesh-spinner" aria-hidden="true"></span>
                        <span v-else>{{ emoji }}</span>
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
            <div class="mesh-chat-compose">
              <div v-if="sendError" class="mesh-chat-send-error">{{ sendError }}</div>
              <div v-if="attachError" class="mesh-chat-send-error">{{ attachError }}</div>
              <div v-if="pendingReply" class="mesh-chat-pending-reply">
                <span class="mesh-typed-icon">↳</span>
                <span class="mesh-chat-pending-name">Replying to: {{ pendingReply.preview }}</span>
                <button class="mesh-chat-pending-clear" @click="clearPendingReply" title="Cancel reply">✕</button>
              </div>
              <div v-if="pendingEdit" class="mesh-chat-pending-reply">
                <span class="mesh-typed-icon">✎</span>
                <span class="mesh-chat-pending-name">Editing: {{ pendingEdit.original_text }}</span>
                <button class="mesh-chat-pending-clear" @click="clearPendingEdit" title="Cancel edit">✕</button>
              </div>
              <div v-if="pendingAttachment" class="mesh-chat-pending-attachment">
                <span class="mesh-typed-icon">📎</span>
                <span class="mesh-chat-pending-name">{{ pendingAttachment.filename || pendingAttachment.mime }}</span>
                <span class="mesh-chat-pending-size">{{ pendingAttachment.size }} B</span>
                <button class="mesh-chat-pending-clear" @click="clearPendingAttachment" title="Discard attachment">✕</button>
              </div>
              <div class="mesh-chat-compose-row">
                <div v-if="activeChatPeer" class="mesh-attach-menu-anchor">
                  <Transition name="mesh-attach-stack">
                    <div v-if="showAttachMenu" class="mesh-attach-stack">
                      <label
                        class="glass-button mesh-chat-attach-btn"
                        :class="{ 'is-busy': attaching }"
                        :title="attaching ? 'uploading…' : 'Attach file'"
                      >
                        <input
                          type="file"
                          @change="(e) => { showAttachMenu = false; handleAttachFile(e) }"
                          style="display:none;"
                          :disabled="attaching"
                        />
                        <span v-if="attaching" class="mesh-spinner" aria-hidden="true"></span>
                        <span v-else>📎</span>
                      </label>
                      <button
                        type="button"
                        class="glass-button mesh-chat-record-btn"
                        :class="{ 'is-recording': isRecordingVoice }"
                        :disabled="attaching"
                        :title="isRecordingVoice ? 'Release to send' : 'Hold to record a voice message'"
                        @pointerdown.prevent="startVoiceRecording"
                        @pointerup.prevent="() => { stopVoiceRecording(); showAttachMenu = false }"
                        @pointerleave="() => { if (isRecordingVoice) { stopVoiceRecordingIfActive(); showAttachMenu = false } }"
                      >
                        <span v-if="isRecordingVoice" class="mesh-spinner" aria-hidden="true"></span>
                        <span v-else>🎤</span>
                      </button>
                    </div>
                  </Transition>
                  <button
                    type="button"
                    class="glass-button mesh-chat-plus-btn"
                    :class="{ 'is-open': showAttachMenu }"
                    title="Attach"
                    @click="showAttachMenu = !showAttachMenu"
                  >
                    +
                  </button>
                </div>
                <input
                  ref="messageInputRef"
                  v-model="messageText"
                  class="mesh-chat-input"
                  :placeholder="activeChatPeer ? 'Type a message or pick a file…' : 'Type a message...'"
                  maxlength="160"
                  @keydown.enter.exact.prevent="handleSendMessage"
                />
                <button
                  class="glass-button mesh-chat-send-btn"
                  :disabled="(!messageText.trim() && !pendingAttachment) || mesh.sending || sendingArch"
                  @click="handleSendMessage"
                >
                  <span v-if="mesh.sending || sendingArch" class="mesh-send-spinner" aria-label="Sending"></span>
                  <template v-else>{{ pendingReply ? 'Reply' : (pendingAttachment ? 'Share' : 'Send') }}</template>
                </button>
              </div>
            </div>
          </template>
        </div>

        <!-- Tools panels (3rd column on wide screens) -->
        <div class="mesh-tools-wrapper" data-controller-zone="mesh-tools">
          <div v-if="isWideDesktop" class="mesh-tools-tab-bar">
            <button class="mesh-tab" :class="{ active: toolsTab === 'bitcoin' }" @click="toolsTab = 'bitcoin'">Bitcoin</button>
            <button class="mesh-tab" :class="{ active: toolsTab === 'deadman' }" @click="toolsTab = 'deadman'">
              Dead Man
              <span v-if="mesh.deadmanStatus?.triggered" class="mesh-tab-badge mesh-tab-badge-alert">!</span>
            </button>
            <button class="mesh-tab" :class="{ active: toolsTab === 'assistant' }" @click="toolsTab = 'assistant'">
              AI
            </button>
            <button class="mesh-tab" :class="{ active: toolsTab === 'map' }" @click="toolsTab = 'map'">Map</button>
            <button class="mesh-tab" :class="{ active: toolsTab === 'device' }" @click="toolsTab = 'device'" title="Device settings">📡</button>
          </div>
          <MeshBitcoinPanel v-if="showBitcoinPanel" />
          <MeshDeadmanPanel v-if="showDeadmanPanel" />
          <MeshAssistantPanel v-if="showAssistantPanel" />
          <div v-if="showMapPanel" class="glass-card mesh-map-panel"><MeshMap /></div>
          <MeshDevicePanel v-if="showDevicePanel" />
        </div>
      </div>

      <!-- Mobile tools: show under peers list on first view -->
      <div v-if="showMobileTools" class="mesh-mobile-tools">
        <div v-if="showMapPanel" class="glass-card mesh-map-panel"><MeshMap /></div>
        <MeshBitcoinPanel v-if="showBitcoinPanel" />
        <MeshDeadmanPanel v-if="showDeadmanPanel" />
        <MeshAssistantPanel v-if="showAssistantPanel" />
        <MeshDevicePanel v-if="showDevicePanel" />
      </div>
    </div>

    <!-- Mobile: floating tab strip pinned above the global tab bar (same
         placement as the mobile back button). Switches the whole pane between
         the chat and each tool. Hidden while an individual conversation is open
         (the back button takes over there). -->
    <Teleport v-if="isViewActive" to="body">
      <div v-show="!mobileShowChat" class="mesh-mobile-tabbar">
        <button class="mesh-mtab" :class="{ active: mobileTab === 'chat' }" @click="selectMobileTab('chat')">Chat</button>
        <button class="mesh-mtab" :class="{ active: mobileTab === 'bitcoin' }" @click="selectMobileTab('bitcoin')">BTC</button>
        <button class="mesh-mtab" :class="{ active: mobileTab === 'deadman' }" @click="selectMobileTab('deadman')">
          Dead Man
          <span v-if="mesh.deadmanStatus?.triggered" class="mesh-tab-badge mesh-tab-badge-alert">!</span>
        </button>
        <button class="mesh-mtab" :class="{ active: mobileTab === 'assistant' }" @click="selectMobileTab('assistant')">AI</button>
        <button class="mesh-mtab" :class="{ active: mobileTab === 'map' }" @click="selectMobileTab('map')">Map</button>
        <button class="mesh-mtab" :class="{ active: mobileTab === 'device' }" @click="selectMobileTab('device')" title="Device settings">📡</button>
      </div>
    </Teleport>

    <!-- Transport chooser modal: shown when attachment size fits both mesh
         (inline-chunked) and Tor. User picks which path to send it over.
         Teleported to body so the fixed backdrop covers the FULL viewport —
         rendered in place it sits inside a transformed/filtered glass panel,
         which traps position:fixed to just the right chat panel. -->
    <Teleport v-if="isViewActive" to="body">
    <div v-if="transportChoice" class="mesh-transport-modal-backdrop" @click.self="pickTransport('cancel')">
      <div class="glass-card mesh-transport-modal">
        <h3 class="mesh-transport-title">📎 How should I send this?</h3>
        <p class="mesh-transport-sub">
          <strong>{{ transportChoice.file.name }}</strong>
          · {{ (transportChoice.size / 1024).toFixed(1) }} KB
        </p>
        <div class="mesh-transport-options">
          <button class="mesh-transport-option" @click="pickTransport('mesh')">
            <span class="mesh-transport-icon">📡</span>
            <span class="mesh-transport-label">Over mesh</span>
            <span class="mesh-transport-meta">~{{ transportChoice.est_seconds }}s · works offline</span>
          </button>
          <button
            class="mesh-transport-option"
            :disabled="!transportChoice.has_tor"
            @click="pickTransport('tor')"
          >
            <span class="mesh-transport-icon">🧅</span>
            <span class="mesh-transport-label">Over Tor</span>
            <span class="mesh-transport-meta">
              {{ transportChoice.has_tor ? 'instant · needs onion' : 'no Tor path to peer' }}
            </span>
          </button>
        </div>
        <button class="mesh-transport-cancel" @click="pickTransport('cancel')">Cancel</button>
      </div>
    </div>
    </Teleport>

    <!-- Image quality-picker modal: shown before sending an image attachment.
         Each preset shows its nominal size target + a transfer-time estimate
         from the same mesh.transport-advice RPC the file-attach flow uses.
         Teleported to body — see the transport chooser above. -->
    <Teleport v-if="isViewActive" to="body">
    <div v-if="imageQualityChoice" class="mesh-transport-modal-backdrop" @click.self="pickImageQuality(null)">
      <div class="glass-card mesh-transport-modal">
        <h3 class="mesh-transport-title">🖼️ Choose Image Quality</h3>
        <p class="mesh-transport-sub">
          <strong>{{ imageQualityChoice.file.name }}</strong>
          · {{ (imageQualityChoice.file.size / 1024).toFixed(1) }} KB original
        </p>
        <div class="mesh-transport-options">
          <button
            v-for="preset in IMAGE_COMPRESSION_PRESETS"
            :key="preset.key"
            class="mesh-transport-option"
            @click="pickImageQuality(preset)"
          >
            <span class="mesh-transport-icon">🖼️</span>
            <span class="mesh-transport-label">{{ preset.displayName }} — {{ preset.description }}</span>
            <span class="mesh-transport-meta">{{
              imageSendTransport === 'lora' ? (imageQualityEstimates.get(preset.key) ?? '…') : 'instant'
            }}</span>
          </button>
        </div>
        <!-- Transport pills: LoRa is always available (it's a mesh chat);
             FIPS/Tor appear when the peer is a reachable federation node,
             letting the user send the image that way instead. -->
        <div
          v-if="imageTransportOptions.fips || imageTransportOptions.tor"
          class="mesh-image-transport-row"
        >
          <span class="mesh-image-transport-caption">Send via</span>
          <button
            class="mesh-image-transport-pill"
            :class="{ active: imageSendTransport === 'lora' }"
            @click="imageSendTransport = 'lora'"
          >📡 LoRa</button>
          <button
            v-if="imageTransportOptions.fips"
            class="mesh-image-transport-pill"
            :class="{ active: imageSendTransport === 'fips' }"
            @click="imageSendTransport = 'fips'"
          >⚡ FIPS</button>
          <button
            v-if="imageTransportOptions.tor"
            class="mesh-image-transport-pill"
            :class="{ active: imageSendTransport === 'tor' }"
            @click="imageSendTransport = 'tor'"
          >🧅 Tor</button>
        </div>
        <button class="mesh-transport-cancel" @click="pickImageQuality(null)">Cancel</button>
      </div>
    </div>
    </Teleport>

    <!-- Hop visualization modal: click a message's transport pill to see how
         it traveled — branded medallion + animated packet design, vertical on
         mobile. Self-contained component (Teleports to body itself). -->
    <HopVizModal
      v-if="hopVizMsg"
      :msg="hopVizMsg"
      :peer="hopVizPeer"
      :transport-label="transportLabel(hopVizMsg)"
      :signal-label="signalQualityLabel(hopVizPeer?.snr ?? null, hopVizPeer?.rssi ?? null)"
      :time-label="timeAgo(hopVizMsg.timestamp)"
      @close="hopVizMsg = null"
    />

    <!-- The "mesh device detected" setup flow is now the global
         MeshDeviceSetupModal mounted in App.vue (fires on every page,
         board image + region presets), so no local modal here. -->

    <MediaLightbox
      :items="lightboxItems"
      :start-index="0"
      :show="lightboxOpen"
      :fetch-blob-url="lightboxFetchBlobUrl"
      @close="lightboxOpen = false"
    />

  </div>
</template>

<!-- Styles extracted to mesh/mesh-styles.css -->
