<template>
  <!-- Map view: no pb-6 — the .dashboard-scroll-panel:has(.node-map-stage)
       rules turn this view into a column that hands remaining height to the
       map, so bottom padding would just re-create the dead margin. -->
  <div :class="mapActive ? undefined : 'pb-6'">
    <FederationHeader
      :self-did="selfDid"
      :server-name="appStore.serverName"
      @rotate="showRotateModal = true"
    />

    <RotateDidModal
      :visible="showRotateModal"
      :rotating="rotatingDid"
      :error="rotateError"
      :success="rotateSuccess"
      @close="showRotateModal = false; rotateError = ''; rotateSuccess = ''"
      @rotate="rotateDid"
    />

    <!-- View Tabs (same style as Home Dashboard/Setup tabs; full-width on mobile) -->
    <!-- md:self-start: in map view the root is a flex column, and stretch
         alignment would otherwise pull the pill full-width on desktop -->
    <div v-if="nodes.length > 0" role="tablist" class="mode-switcher mb-6 w-full md:w-auto md:self-start">
      <button
        v-for="tab in viewTabs"
        :key="tab.id"
        class="mode-switcher-btn"
        role="tab"
        :aria-selected="activeView === tab.id"
        :class="{ 'mode-switcher-btn-active': activeView === tab.id }"
        @click="setView(tab.id)"
      >
        {{ tab.label }}
      </button>
    </div>

    <!-- Mobile DID card: below the tabs per UX; hidden on the map tab where
         vertical space belongs to the map (desktop keeps the header card) -->
    <DidCardMobile
      v-if="!mapActive"
      :self-did="selfDid"
      :server-name="appStore.serverName"
      @rotate="showRotateModal = true"
    />

    <!-- Network Map View — fills all remaining height to the bottom edge -->
    <div v-if="mapActive" class="flex-1 min-h-0">
      <NetworkMap3D
        :nodes="mapNodes"
        :links="mapLinks"
        :requests="mapRequests"
        @select="onMapSelect"
        @approve="approvePending"
        @reject="rejectPending"
      />
    </div>

    <template v-if="activeView === 'list'">
      <QuickActions
        :generating-invite="generatingInvite"
        :invite-type="inviteType"
        :invite-code="inviteCode"
        :syncing="syncing"
        @generate-invite="handleGenerateInvite"
        @show-join="showJoinModal = true"
        @sync="syncAll"
        @clear-invite="inviteCode = ''"
      />

      <!-- Nostr discoverability strip: opt-in toggle + Discover button.
           Renders inline so the Federation page is the single place a user
           manages everything related to peering. The toggle directly mutates
           the `nostr_discovery_enabled` config flag — backend defaults to OFF
           and nothing is published until the user explicitly turns it on. -->
      <div class="glass-card p-4 mb-6">
        <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <span class="text-sm font-medium text-white">Nostr discoverability</span>
              <span
                class="inline-block px-2 py-0.5 text-[10px] uppercase tracking-wide rounded"
                :class="discoveryEnabled ? 'bg-green-500/20 text-green-300' : 'bg-white/10 text-white/50'"
              >{{ discoveryEnabled ? 'On' : 'Off' }}</span>
            </div>
            <p class="text-xs text-white/60 mt-1">
              When on, this node publishes a presence event (DID + npub only — never an onion)
              so other nodes can find you and request to peer. Inbound requests land in the
              panel below for your approval. Off by default.
            </p>
            <button
              class="text-xs text-white/50 hover:text-white/80 underline underline-offset-2 mt-1"
              @click="showSigningInfo = !showSigningInfo"
            >
              {{ showSigningInfo ? 'Hide signing details' : 'Signing details' }}
            </button>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <button
              class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50"
              :disabled="discoveryToggling"
              @click="toggleDiscovery"
            >
              {{ discoveryToggling ? '…' : (discoveryEnabled ? 'Disable' : 'Enable') }}
            </button>
            <button
              class="px-3 py-1.5 glass-button glass-button-sm rounded text-xs text-white/90 hover:text-white disabled:opacity-50"
              :disabled="!discoveryEnabled"
              @click="showDiscoverModal = true"
            >
              Discover Nodes
            </button>
          </div>
        </div>

        <!-- Signing details: shows WHO signs the presence event (always the
             node's own discovery key — deliberately not pickable, so a user's
             personal Web5/Nostr identities can never be burned for node
             discovery) and a human-readable view of WHAT gets signed. -->
        <div v-if="showSigningInfo" class="mt-4 pt-4 border-t border-white/10 space-y-3">
          <div>
            <label class="block text-[10px] uppercase tracking-wide text-white/40 mb-1">Signer</label>
            <!-- Same row style as the NostrIdentityPicker overlay, but locked:
                 node discovery always signs with the node's own key, never a
                 personal identity, so there is nothing to pick. -->
            <div class="w-full sm:max-w-md p-3 rounded-lg bg-white/[0.03] ring-1 ring-white/10">
              <div class="flex items-center gap-3">
                <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0 bg-white/10 text-white/80">
                  <span class="text-sm font-bold">{{ (appStore.serverName || 'N').charAt(0).toUpperCase() }}</span>
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-white font-semibold text-sm truncate">{{ appStore.serverName || 'This node' }}</span>
                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-white/10 text-white/60">node identity</span>
                  </div>
                  <div class="mt-0.5">
                    <span class="text-white/35 text-xs font-mono truncate">{{ nodeNpub ? truncateNpub(nodeNpub) : 'loading…' }}</span>
                  </div>
                </div>
                <svg class="w-4 h-4 text-white/30 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2" aria-label="Locked">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 10-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 002.25-2.25v-6.75a2.25 2.25 0 00-2.25-2.25H6.75a2.25 2.25 0 00-2.25 2.25v6.75a2.25 2.25 0 002.25 2.25z" />
                </svg>
              </div>
            </div>
            <p class="text-[11px] text-white/40 mt-1">
              Presence events are always signed with this node's dedicated discovery key.
              Your personal identities are never used for node discovery, so unlike the app
              sign-in overlay there is nothing to choose here.
            </p>
          </div>
          <div>
            <label class="block text-[10px] uppercase tracking-wide text-white/40 mb-1">What gets signed &amp; published</label>
            <dl class="text-xs space-y-1.5">
              <div class="flex flex-col sm:flex-row sm:gap-2">
                <dt class="text-white/50 shrink-0 sm:w-28">Identity (DID)</dt>
                <dd class="text-white/80 font-mono break-all">{{ selfDid || '—' }}</dd>
              </div>
              <div class="flex flex-col sm:flex-row sm:gap-2">
                <dt class="text-white/50 shrink-0 sm:w-28">Signing key</dt>
                <dd class="text-white/80 font-mono break-all">{{ nodeNpub || '—' }}</dd>
              </div>
              <div class="flex flex-col sm:flex-row sm:gap-2">
                <dt class="text-white/50 shrink-0 sm:w-28">Software version</dt>
                <dd class="text-white/80">{{ appVersion || '—' }}</dd>
              </div>
              <div class="flex flex-col sm:flex-row sm:gap-2">
                <dt class="text-white/50 shrink-0 sm:w-28">Event format</dt>
                <dd class="text-white/80">Nostr kind 30078, replaceable (NIP-33), tag <span class="font-mono">archipelago-node</span></dd>
              </div>
            </dl>
            <p class="text-[11px] text-white/40 mt-2">
              Every presence event carries a Schnorr signature (NIP-01) made with the key
              above — relays reject unsigned events, so anything you see via discovery was
              cryptographically signed by its node. Your onion address is never part of this
              event; it is only shared over encrypted DMs (NIP-44) after you approve a peer.
            </p>
          </div>
        </div>
      </div>
      <div v-if="discoveryError" class="mb-4 text-xs text-red-400">{{ discoveryError }}</div>

      <PendingRequestsPanel
        :requests="pendingRequests"
        :peer-dids="nodes.map(node => node.did)"
        :polling="pollingHandshake"
        :busy-id="pendingBusyId"
        @poll="pollHandshake"
        @approve="approvePending"
        @reject="rejectPending"
        @cancel="cancelPending"
      />

      <NodeList
        :nodes="nodes"
        :loading="loading"
        :error="error"
        :sync-results="syncResults"
        :dwn-sync-dot-class="dwnSyncDotClass"
        :cleaning-nodes="cleaningNodes"
        @select-node="selectedNode = $event"
        @clear-sync-results="syncResults = []"
        @cleanup-dead="cleanupDeadNodes"
      />
    </template>

    <NodeDetailModal
      :node="selectedNode"
      :dwn-sync-dot-class="dwnSyncDotClass"
      :dwn-sync-label="dwnSyncLabel"
      :dwn-message-count="String(dwnStatus?.message_count ?? '--')"
      :dwn-last-sync="dwnStatus?.last_sync ? timeAgo(dwnStatus.last_sync) : 'never'"
      :dwn-syncing="dwnSyncing"
      :deploying="deploying"
      :deploy-result="deployResult"
      :action-error="nodeActionError"
      @close="selectedNode = null; nodeActionError = ''"
      @change-trust="changeTrust"
      @remove-node="removeNode"
      @deploy-app="deployApp"
      @dwn-sync="triggerDwnSync"
    />

    <JoinModal
      :visible="showJoinModal"
      :joining="joining"
      :error="joinError"
      :success="joinSuccess"
      @close="showJoinModal = false"
      @join="joinFederation"
    />

    <DiscoverModal
      :visible="showDiscoverModal"
      :outbound-sent="pendingRequests"
      @close="showDiscoverModal = false"
      @sent="loadPendingRequests"
    />

    <PresenceSignModal
      :show="showPresenceSignModal"
      :server-name="appStore.serverName"
      :npub="nodeNpub"
      :did="selfDid"
      :version="appVersion"
      :busy="discoveryToggling"
      @confirm="confirmPresenceSign"
      @cancel="showPresenceSignModal = false"
    />

    <TrustPasswordModal
      :visible="showTrustPassword"
      :context="trustPasswordContext"
      :busy="trustPasswordBusy"
      :error="trustPasswordError"
      @confirm="submitTrustPassword"
      @close="closeTrustPassword"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { validContact, lastContactLabel } from './web5/nodeAvailability'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import { useTransportStore } from '@/stores/transport'
import { useAppStore } from '@/stores/app'
import { useSyncStore } from '@/stores/sync'
import NetworkMap3D from '@/components/federation/NetworkMap3D.vue'
import FederationHeader from './federation/FederationHeader.vue'
import DidCardMobile from './federation/DidCardMobile.vue'
import RotateDidModal from './federation/RotateDidModal.vue'
import QuickActions from './federation/QuickActions.vue'
import NodeList from './federation/NodeList.vue'
import NodeDetailModal from './federation/NodeDetailModal.vue'
import JoinModal from './federation/JoinModal.vue'
import PendingRequestsPanel from './federation/PendingRequestsPanel.vue'
import DiscoverModal from './federation/DiscoverModal.vue'
import PresenceSignModal from './federation/PresenceSignModal.vue'
import TrustPasswordModal from './federation/TrustPasswordModal.vue'
import type { FederatedNode, DwnStatus, SyncResult } from './federation/types'
import type { PendingPeerRequest } from '@/api/rpc-client'
import { nodeName, nodeNameFromDid, timeAgo } from './federation/utils'

const transportStore = useTransportStore()
const appStore = useAppStore()
const syncStore = useSyncStore()

// Cached: revisits paint the node list instantly; the 5s poll and mutation
// refreshes revalidate behind it. `loading` is initial-load only (background
// refreshes keep content on screen — the old showLoader:false semantics).
const nodesRes = useCachedResource<FederatedNode[]>({
  key: 'federation.nodes',
  fetcher: async () => (await rpcClient.federationListNodes()).nodes,
  persist: false, // FederatedNode carries did/pubkey/onion — peer identity payload (T-02-01)
})
const nodes = computed(() => nodesRes.data.value ?? [])
const loading = computed(() => nodesRes.loadState.value === 'loading')
const error = ref('')
/** Failure from an action taken inside NodeDetailModal (trust dropdown). Kept
 *  separate from `error`, which renders in NodeList behind the modal. */
const nodeActionError = ref('')
const selectedNode = ref<FederatedNode | null>(null)
const inviteType = ref<'trusted' | 'observer'>('trusted')

const inviteCode = ref('')
const generatingInvite = ref(false)

const showJoinModal = ref(false)
const joining = ref(false)
const joinError = ref('')
const joinSuccess = ref(false)

const syncing = ref(false)
const syncResults = ref<SyncResult[]>([])

const deploying = ref(false)
const deployResult = ref('')

const viewTabs = [
  { id: 'list', label: 'List View' },
  { id: 'map', label: 'Network Map' },
] as const

type ViewId = typeof viewTabs[number]['id']
const activeView = ref<ViewId>(
  (localStorage.getItem('federation-view') as ViewId) || (nodes.value.length >= 3 ? 'map' : 'list')
)

function setView(id: ViewId) {
  activeView.value = id
  localStorage.setItem('federation-view', id)
}

const mapActive = computed(() => activeView.value === 'map' && nodes.value.length > 0)

/** Map click-through: tapping a peer opens the same detail modal as the list
 *  view. Tapping the self node is a no-op (its actions live in the header). */
function onMapSelect(did: string) {
  const node = nodes.value.find(n => n.did === did)
  if (node) selectedNode.value = node
}

/** Seeded from the cached DID so the map's centre node (and its links) exist
 *  on the very first frame; the authoritative fetch in onMounted refreshes it
 *  and re-caches. Without this the intro raced the RPC and often played with
 *  no centre. */
const selfDid = ref<string>((() => {
  try { return localStorage.getItem('neode_did') || '' } catch { return '' }
})())

const availabilityNow = ref(Date.now())
const mapNodes = computed(() => {
  const result = []
  if (selfDid.value) {
    result.push({
      did: selfDid.value,
      label: appStore.serverName,
      trust_level: 'trusted' as const,
      online: true,
      app_count: 0,
      is_self: true,
    })
  }
  for (const node of nodes.value) {
    result.push({
      did: node.did,
      label: nodeName(node),
      trust_level: node.trust_level as 'trusted' | 'observer' | 'untrusted',
      online: isOnlineCheck(node),
      statusLabel: isOnlineCheck(node) ? 'Online' : lastContactLabel(node.last_seen, availabilityNow.value),
      app_count: node.last_state?.apps?.length ?? 0,
      is_self: false,
    })
  }
  return result
})

const mapLinks = computed(() => {
  if (!selfDid.value) return []
  return nodes.value.map(n => ({
    source: selfDid.value,
    target: n.did,
  }))
})

/** Inbound pending requests for the map — blinking yellow nodes the user can
 *  accept/reject in place (same RPCs as the pending panel). */
const mapRequests = computed(() => pendingRequests.value
  .filter(r => !r.outbound && r.state === 'pending')
  .map(r => ({
    id: r.id,
    label: r.from_name || `${r.from_nostr_npub.slice(0, 12)}…`,
    message: r.message,
  })))

const dwnStatusRes = useCachedResource<DwnStatus>({
  key: 'federation.dwn-status',
  fetcher: (signal) => rpcClient.call<DwnStatus>({ method: 'dwn.status', signal, dedup: true, maxRetries: 1 }),
  persist: true, // sync status/counters only — no identity payload
  immediate: false,
})
const dwnStatus = computed(() => dwnStatusRes.data.value)
const dwnSyncing = ref(false)

const dwnSyncDotClass = computed(() => {
  if (!dwnStatus.value) return 'bg-white/30'
  switch (dwnStatus.value.sync_status) {
    case 'synced': return 'bg-green-400'
    case 'syncing': return 'bg-yellow-400 animate-pulse'
    case 'error': return 'bg-red-400'
    default: return 'bg-white/30'
  }
})

const dwnSyncLabel = computed(() => {
  if (!dwnStatus.value) return 'Unknown'
  switch (dwnStatus.value.sync_status) {
    case 'synced': return 'Synced'
    case 'syncing': return 'Syncing...'
    case 'error': return 'Error'
    default: return dwnStatus.value.sync_status || 'Unknown'
  }
})

// DID rotation
const showRotateModal = ref(false)
const rotatingDid = ref(false)
const rotateError = ref('')
const rotateSuccess = ref('')

// Dead node cleanup
const cleaningNodes = ref(false)

// Nostr discoverability + pending peer requests
const discoveryEnabled = ref(false)
const discoveryToggling = ref(false)
const discoveryError = ref('')
const showDiscoverModal = ref(false)
// Signing-details disclosure: who signs the presence event (always the node's
// own discovery key) and a human-readable view of the signed content.
const showSigningInfo = ref(false)
const showPresenceSignModal = ref(false)
const nodeNpub = ref('')
const appVersion = computed(() => syncStore.serverInfo?.version || '')

function truncateNpub(npub: string): string {
  if (npub.length <= 20) return npub
  return npub.slice(0, 12) + '...' + npub.slice(-6)
}

async function loadNodeNpub() {
  try {
    const res = await rpcClient.call<{ nostr_npub?: string }>({ method: 'node.nostr-pubkey' })
    if (res?.nostr_npub) nodeNpub.value = res.nostr_npub
  } catch {
    // Nostr key not provisioned yet — panel shows a placeholder
  }
}
const pendingRequests = ref<PendingPeerRequest[]>([])
const pollingHandshake = ref(false)
const pendingBusyId = ref<string | null>(null)

async function loadDiscoveryState() {
  try {
    const result = await rpcClient.nostrDiscoveryStatus()
    discoveryEnabled.value = !!result.enabled
  } catch {
    discoveryEnabled.value = false
  }
}

async function toggleDiscovery() {
  if (!discoveryEnabled.value) {
    // Enabling publishes a signed presence event — surface the signer overlay
    // first so the user sees exactly what gets signed and with which key.
    // Nothing is published until they confirm.
    if (!nodeNpub.value) loadNodeNpub()
    showPresenceSignModal.value = true
    return
  }
  // Disabling publishes nothing new — apply immediately.
  discoveryToggling.value = true
  discoveryError.value = ''
  try {
    const result = await rpcClient.nostrSetDiscovery(false)
    discoveryEnabled.value = !!result.enabled
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Failed to toggle discoverability'
  } finally {
    discoveryToggling.value = false
  }
}

async function confirmPresenceSign() {
  discoveryToggling.value = true
  discoveryError.value = ''
  try {
    const result = await rpcClient.nostrSetDiscovery(true)
    discoveryEnabled.value = !!result.enabled
    showPresenceSignModal.value = false
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Failed to enable discoverability'
  } finally {
    discoveryToggling.value = false
  }
}

async function loadPendingRequests() {
  try {
    const result = await rpcClient.federationListPendingRequests()
    pendingRequests.value = result.requests
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Failed to load pending requests'
  }
}

async function pollHandshake() {
  pollingHandshake.value = true
  discoveryError.value = ''
  try {
    await rpcClient.handshakePoll()
    await loadPendingRequests()
    // If a poll applied a PeerInvite, the federation node list also changed.
    await loadNodes()
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Poll failed'
  } finally {
    pollingHandshake.value = false
  }
}

async function approvePending(id: string) {
  pendingBusyId.value = id
  discoveryError.value = ''
  try {
    await rpcClient.federationApproveRequest(id)
    await loadPendingRequests()
    await loadNodes()
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Approve failed'
  } finally {
    pendingBusyId.value = null
  }
}

async function rejectPending(id: string) {
  pendingBusyId.value = id
  discoveryError.value = ''
  try {
    await rpcClient.federationRejectRequest(id)
    await loadPendingRequests()
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Reject failed'
  } finally {
    pendingBusyId.value = null
  }
}

async function cancelPending(id: string) {
  pendingBusyId.value = id
  discoveryError.value = ''
  try {
    // Default notify=true from the rpc-client — the peer's inbound row
    // disappears from their UI so they don't have to wonder about a
    // stale handshake.
    await rpcClient.federationCancelRequest(id)
    await loadPendingRequests()
  } catch (e: unknown) {
    discoveryError.value = e instanceof Error ? e.message : 'Cancel failed'
  } finally {
    pendingBusyId.value = null
  }
}

function isOnlineCheck(node: FederatedNode): boolean {
  const lastSeen = validContact(node.last_seen, availabilityNow.value)
  return lastSeen !== undefined && availabilityNow.value - lastSeen < 10 * 60 * 1000
}

/** Explicit reload (mutations, retry): surfaces a load failure in the error
 *  banner. The background poll calls nodesRes.refresh() directly and stays
 *  silent, like the old surfaceErrors:false path. */
async function loadNodes() {
  await nodesRes.refresh()
  if (nodesRes.error.value) error.value = nodesRes.error.value
  else error.value = ''
}

function handleGenerateInvite(type: 'trusted' | 'observer') {
  inviteType.value = type
  generateInvite()
}

/** The backend is the only authority on whether a given change is an
 *  escalation, so the UI never pre-judges: it attempts the call and prompts
 *  only when the backend says a password is required. That keeps demotions —
 *  and no-op re-sets of an already-Trusted peer — free of a pointless prompt
 *  without the frontend having to duplicate the rule. */
function isPasswordRequired(e: unknown): boolean {
  return e instanceof Error && e.message.includes('PASSWORD_REQUIRED')
}

const showTrustPassword = ref(false)
const trustPasswordContext = ref('')
const trustPasswordBusy = ref(false)
const trustPasswordError = ref('')
let pendingTrustAction: ((password: string) => Promise<void>) | null = null

function promptForTrustPassword(context: string, action: (password: string) => Promise<void>) {
  trustPasswordContext.value = context
  trustPasswordError.value = ''
  pendingTrustAction = action
  showTrustPassword.value = true
}

function closeTrustPassword() {
  showTrustPassword.value = false
  trustPasswordError.value = ''
  trustPasswordBusy.value = false
  pendingTrustAction = null
}

async function submitTrustPassword(password: string) {
  if (!pendingTrustAction) return
  try {
    trustPasswordBusy.value = true
    trustPasswordError.value = ''
    await pendingTrustAction(password)
    closeTrustPassword()
  } catch (e) {
    // Keep the failure inside the modal so the operator can retry in place
    // rather than losing the pending action to the page-level banner.
    trustPasswordError.value = e instanceof Error ? e.message : 'Password verification failed'
  } finally {
    trustPasswordBusy.value = false
  }
}

/** Raw call — throws so both the first attempt and the password retry can
 *  route the error to the right place. */
async function requestInvite(password?: string) {
  // The invite type is not cosmetic: it sets the trust level the invite
  // grants both sides ("Invite a Peer" = observer, "Link Your Nodes" = trusted)
  const result = await rpcClient.federationInvite(inviteType.value, password)
  inviteCode.value = result.code
}

async function generateInvite() {
  try {
    generatingInvite.value = true
    error.value = ''
    await requestInvite()
  } catch (e) {
    if (isPasswordRequired(e)) {
      promptForTrustPassword(
        'This invite grants Trusted access to whoever redeems it — full read of this node\'s state, and the ability to deploy apps to it. Confirm with your node password.',
        requestInvite,
      )
      return
    }
    error.value = e instanceof Error ? e.message : 'Failed to generate invite'
  } finally {
    generatingInvite.value = false
  }
}

async function joinFederation(code: string) {
  try {
    joining.value = true
    joinError.value = ''
    joinSuccess.value = false
    await rpcClient.federationJoin(code)
    joinSuccess.value = true
    await loadNodes()
    setTimeout(() => { showJoinModal.value = false; joinSuccess.value = false }, 1500)
  } catch (e) {
    joinError.value = e instanceof Error ? e.message : 'Failed to join'
  } finally {
    joining.value = false
  }
}

async function syncAll() {
  try {
    syncing.value = true
    error.value = ''
    syncResults.value = []
    const result = await rpcClient.call<{
      synced: number; failed: number;
      results: SyncResult[]
    }>({ method: 'federation.sync-state', timeout: 180000 })
    syncResults.value = result.results
    await loadNodes()
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Sync failed — some peers may be unreachable over Tor'
  } finally {
    syncing.value = false
  }
}

/** Raw call — throws; see `requestInvite`. */
async function requestTrustChange(did: string, level: string, password?: string) {
  await rpcClient.federationSetTrust(did, level as 'trusted' | 'observer' | 'untrusted', password)
  await loadNodes()
  if (selectedNode.value?.did === did) {
    selectedNode.value = nodes.value.find(n => n.did === did) ?? null
  }
}

async function changeTrust(did: string, level: string) {
  nodeActionError.value = ''
  try {
    await requestTrustChange(did, level)
  } catch (e) {
    if (isPasswordRequired(e)) {
      const name = nodeNameFromDid(did, nodes.value)
      promptForTrustPassword(
        `Granting ${name} Trusted lets it read this node's state and deploy apps to it. Confirm with your node password.`,
        (password) => requestTrustChange(did, level, password),
      )
      return
    }
    // Show the failure INSIDE the open modal. `error` renders in NodeList,
    // which sits behind NodeDetailModal — so routing it there made a failed
    // trust change look like the dropdown simply doing nothing.
    const msg = e instanceof Error ? e.message : 'Failed to update trust level'
    if (selectedNode.value?.did === did) nodeActionError.value = msg
    else error.value = msg
  }
}

async function removeNode(did: string) {
  try {
    await rpcClient.federationRemoveNode(did)
    selectedNode.value = null
    await loadNodes()
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to remove node'
  }
}

async function deployApp(did: string, appId: string) {
  try {
    deploying.value = true
    deployResult.value = ''
    await rpcClient.federationDeployApp({ did, appId })
    deployResult.value = `Successfully deployed ${appId} to remote node`
  } catch (e) {
    deployResult.value = `Error: ${e instanceof Error ? e.message : 'Deploy failed'}`
  } finally {
    deploying.value = false
  }
}

function loadDwnStatus() {
  return dwnStatusRes.refresh()
}

async function triggerDwnSync() {
  try {
    dwnSyncing.value = true
    await rpcClient.call({ method: 'dwn.sync', timeout: 120000 })
    await loadDwnStatus()
  } catch {
    // Silently handle sync errors
  } finally {
    dwnSyncing.value = false
  }
}

async function cleanupDeadNodes() {
  cleaningNodes.value = true
  try {
    const deadNodes = nodes.value.filter(n => !isOnlineCheck(n) && (!n.last_seen || n.last_seen === 'never'))
    for (const node of deadNodes) {
      await rpcClient.federationRemoveNode(node.did)
    }
    await loadNodes()
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Cleanup failed'
  } finally {
    cleaningNodes.value = false
  }
}

async function rotateDid(password: string) {
  if (!password) return
  rotatingDid.value = true
  rotateError.value = ''
  rotateSuccess.value = ''
  try {
    const result = await rpcClient.call<{
      old_did: string; new_did: string; proof_signature: string; proof_message: string
    }>({ method: 'node.rotate-did', params: { password } })

    selfDid.value = result.new_did
    try { localStorage.setItem('neode_did', result.new_did) } catch { /* noop */ }
    rotateSuccess.value = `DID rotated. Notifying peers...`

    const notify = await rpcClient.call<{ notified: number; failed: number }>({
      method: 'federation.notify-did-change',
      params: {
        old_did: result.old_did,
        new_did: result.new_did,
        proof_signature: result.proof_signature,
        proof_message: result.proof_message,
      },
      timeout: 120000,
    })
    rotateSuccess.value = `DID rotated successfully. ${notify.notified} peers notified${notify.failed > 0 ? `, ${notify.failed} failed` : ''}.`
  } catch (err: unknown) {
    rotateError.value = err instanceof Error ? err.message : 'Rotation failed'
  } finally {
    rotatingDid.value = false
  }
}

let autoRefreshTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  loadDwnStatus()
  loadDiscoveryState()
  loadPendingRequests()
  loadNodeNpub()
  transportStore.fetchPeers()
  try {
    const result = await rpcClient.getNodeDid()
    selfDid.value = result.did
    try { localStorage.setItem('neode_did', result.did) } catch { /* private mode */ }
  } catch {
    // Self DID not available
  }
  autoRefreshTimer = setInterval(() => {
    availabilityNow.value = Date.now()
    void nodesRes.refresh()
    loadPendingRequests()
  }, 5000)
})

onUnmounted(() => {
  if (autoRefreshTimer) {
    clearInterval(autoRefreshTimer)
    autoRefreshTimer = null
  }
})
</script>
