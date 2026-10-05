<template>
  <div class="pb-6">
    <!-- Quick Actions + HW Banner -->
    <Web5QuickActions
      :showStagger="showStagger"
      :profitsBreakdown="profitsBreakdown"
      :networkingProfitsDisplay="networkingProfitsDisplay"
      :userDid="userDid"
      :didStatus="didStatus"
      :didCopied="didCopied"
      :creatingDid="creatingDid"
      :dhtDid="dhtDid"
      :dhtDidCopied="dhtDidCopied"
      :publishingDht="publishingDht"
      :nostrRelayStats="nostrRelaysRef?.nostrRelayStats ?? null"
      :connectedNodesCount="connectedNodesRef?.peers?.length ?? 0"
      :detectedHwWallets="detectedHwWallets"
      @copyDid="copyDid"
      @showDidDocument="showDidDocument"
      @createDid="createDID"
      @copyDhtDid="copyDhtDid"
      @refreshDhtDid="refreshDhtDid"
      @publishDhtDid="publishDhtDid"
      @manageRelays="nostrRelaysRef?.openRelaysModal()"
    />

    <!-- DID Document Modal -->
    <Teleport to="body">
      <div v-if="showDidDocModal" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-md" @click.self="showDidDocModal = false" @keydown.escape="showDidDocModal = false">
        <div class="glass-card p-6 max-w-2xl w-full max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="did-doc-title">
          <div class="flex items-center justify-between mb-4">
            <h3 id="did-doc-title" class="text-lg font-semibold text-white">{{ t('web5.didDocument') }}</h3>
            <div class="flex items-center gap-2">
              <span v-if="didDocVerified === true" class="text-xs text-green-400 flex items-center gap-1">
                <svg class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd"/></svg>
                {{ t('web5.verified') }}
              </span>
              <span v-else-if="didDocVerified === false" class="text-xs text-red-400">{{ t('web5.invalid') }}</span>
            </div>
          </div>
          <div v-if="loadingDidDoc" class="text-white/60 text-sm">{{ t('common.loading') }}</div>
          <pre v-else class="text-xs text-white/80 font-mono bg-black/30 rounded-lg p-4 overflow-x-auto whitespace-pre-wrap">{{ didDocumentFormatted }}</pre>
          <div class="flex gap-3 mt-4">
            <button @click="copyDidDocument" class="flex-1 px-4 py-2 glass-button rounded-lg text-sm font-medium text-white/90 hover:text-white transition-colors">
              {{ didDocCopied ? t('common.copiedBang') : t('common.copy') }}
            </button>
            <button @click="showDidDocModal = false" class="px-4 py-2 rounded-lg bg-white/10 text-white font-medium hover:bg-white/20 transition-colors">
              {{ t('common.close') }}
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Connected Nodes + Node Visibility -->
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-6 mb-6">
      <Web5ConnectedNodes ref="connectedNodesRef" @toast="showToast" />
      <Web5NodeVisibility :showStagger="showStagger" ref="nodeVisibilityRef" @toast="showToast" />
    </div>

    <!-- Identities + Nostr Relays -->
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-6 mb-6">
      <Web5Identities ref="identitiesRef" :showStagger="showStagger" @toast="showToast" />
      <Web5NostrRelays ref="nostrRelaysRef" :showStagger="showStagger" />
    </div>

    <!-- Monitoring + Federation -->
    <div class="grid grid-cols-1 xl:grid-cols-2 gap-6 mb-8">
      <Web5Monitoring />
      <Web5Federation />
    </div>

    <!-- Send/Receive Modals hidden — wallet card removed -->
    <!-- <Web5SendReceiveModals ref="sendReceiveRef" @toast="showToast" @balancesChanged="reloadBalances" /> -->

    <!-- Identity Toast -->
    <Transition name="content-fade">
      <div v-if="identityToastVisible" class="fixed bottom-24 md:bottom-8 left-1/2 -translate-x-1/2 z-50 px-4 py-2 rounded-lg bg-black/80 backdrop-blur-md border border-white/10 text-white text-sm shadow-lg">
        {{ identityToastText }}
      </div>
    </Transition>
  </div>
</template>

<script lang="ts">
let web5AnimationDone = false
</script>

<script setup lang="ts">
import { ref, computed, onActivated, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import { safeClipboardWrite } from './utils'
import type { ProfitsData, HwWalletDevice } from './types'

import Web5QuickActions from './Web5QuickActions.vue'
// import Web5Wallet from './Web5Wallet.vue'  // hidden for now
// import Web5Domains from './Web5Domains.vue'  // hidden for now
import Web5NostrRelays from './Web5NostrRelays.vue'
import Web5NodeVisibility from './Web5NodeVisibility.vue'
import Web5ConnectedNodes from './Web5ConnectedNodes.vue'
// import Web5SharedContent from './Web5SharedContent.vue'  // hidden for now
import Web5Identities from './Web5Identities.vue'
// import Web5CredentialsSummary from './Web5CredentialsSummary.vue'  // hidden for now
import Web5Monitoring from './Web5Monitoring.vue'
import Web5Federation from './Web5Federation.vue'
// import Web5SendReceiveModals from './Web5SendReceiveModals.vue'  // wallet hidden

const { t } = useI18n()

const showStagger = !web5AnimationDone

// Child refs
// const domainsRef = ref(null)  // hidden for now
const nostrRelaysRef = ref<InstanceType<typeof Web5NostrRelays> | null>(null)
const nodeVisibilityRef = ref<InstanceType<typeof Web5NodeVisibility> | null>(null)
const connectedNodesRef = ref<InstanceType<typeof Web5ConnectedNodes> | null>(null)
const identitiesRef = ref<InstanceType<typeof Web5Identities> | null>(null)
// const credentialsRef = ref(null)  // hidden for now
// const sharedContentRef = ref(null)  // hidden for now
// const sendReceiveRef = ref(null)  // wallet hidden

// --- Toast ---
const identityToastText = ref('')
const identityToastVisible = ref(false)
let identityToastTimer: ReturnType<typeof setTimeout> | undefined

function showToast(text: string) {
  identityToastText.value = text
  identityToastVisible.value = true
  clearTimeout(identityToastTimer)
  identityToastTimer = setTimeout(() => { identityToastVisible.value = false }, 2000)
}

// --- Networking Profits ---
const profitsRes = useCachedResource<ProfitsData>({
  key: 'web5.networking-profits',
  fetcher: (signal) => rpcClient.call<ProfitsData>({ method: 'wallet.networking-profits', signal, dedup: true, maxRetries: 1 }),
  persist: false, // routing/content-sale profit totals — financial data (T-02-01)
})
const profitsBreakdown = computed<ProfitsData | null>(() =>
  profitsRes.data.value
    ?? (profitsRes.error.value ? { total_sats: 0, content_sales_sats: 0, routing_fees_sats: 0 } : null))
const networkingProfitsDisplay = computed(() => {
  if (!profitsBreakdown.value) return '...'
  const sats = profitsBreakdown.value.total_sats
  if (sats === 0) return '0 sats'
  if (sats < 100000) return `${sats.toLocaleString()} sats`
  const btc = sats / 100_000_000
  return `\u20BF${btc.toFixed(8).replace(/0+$/, '').replace(/\.$/, '')}`
})

// --- DID State ---
const storedDid = ref<string | null>(null)
try {
  storedDid.value = localStorage.getItem('neode_did') || null
} catch { /* noop */ }

const userDid = computed(() => storedDid.value)
const didStatus = computed<'active' | 'inactive' | 'pending'>(() => userDid.value ? 'active' : 'inactive')
const creatingDid = ref(false)
const didCopied = ref(false)

// did:dht
const dhtDid = ref<string | null>(null)
const publishingDht = ref(false)
const dhtDidCopied = ref(false)
try {
  dhtDid.value = localStorage.getItem('neode_dht_did') || null
} catch { /* noop */ }

async function createDID() {
  creatingDid.value = true
  try {
    const res = await rpcClient.call<{ did: string }>({ method: 'identity.create-did' })
    storedDid.value = res.did
    localStorage.setItem('neode_did', res.did)
  } catch {
    if (!crypto.subtle) {
      const randomBytes = new Uint8Array(32)
      crypto.getRandomValues(randomBytes)
      const hex = Array.from(randomBytes).map(b => b.toString(16).padStart(2, '0')).join('')
      const did = `did:key:z${hex}`
      storedDid.value = did
      localStorage.setItem('neode_did', did)
    } else {
      const keyPair = await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify'])
      const exported = await crypto.subtle.exportKey('raw', keyPair.publicKey)
      const bytes = new Uint8Array(exported)
      const hex = Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('')
      const did = `did:key:z${hex}`
      storedDid.value = did
      localStorage.setItem('neode_did', did)
    }
  } finally {
    creatingDid.value = false
  }
}

async function copyDid() {
  if (!userDid.value) return
  await safeClipboardWrite(userDid.value)
  didCopied.value = true
  setTimeout(() => { didCopied.value = false }, 2000)
}

async function publishDhtDid() {
  publishingDht.value = true
  try {
    const identities = await rpcClient.call<{ identities: Array<{ id: string; is_default: boolean }> }>({ method: 'identity.list' })
    const defaultId = identities.identities?.find((i: { is_default: boolean }) => i.is_default)
    if (!defaultId) return
    const res = await rpcClient.call<{ dht_did: string }>({
      method: 'identity.create-dht-did',
      params: { identity_id: defaultId.id },
    })
    dhtDid.value = res.dht_did
    localStorage.setItem('neode_dht_did', res.dht_did)
  } catch {
    const did = storedDid.value || localStorage.getItem('neode_did')
    if (did) {
      const dhtVersion = did.replace('did:key:', 'did:dht:')
      dhtDid.value = dhtVersion
      localStorage.setItem('neode_dht_did', dhtVersion)
    }
  } finally {
    publishingDht.value = false
  }
}

async function refreshDhtDid() {
  publishingDht.value = true
  try {
    const identities = await rpcClient.call<{ identities: Array<{ id: string; is_default: boolean }> }>({ method: 'identity.list' })
    const defaultId = identities.identities?.find((i: { is_default: boolean }) => i.is_default)
    if (!defaultId) return
    await rpcClient.call({ method: 'identity.refresh-dht-did', params: { identity_id: defaultId.id } })
  } catch { /* silently ignore */ }
  finally { publishingDht.value = false }
}

async function copyDhtDid() {
  if (!dhtDid.value) return
  await safeClipboardWrite(dhtDid.value)
  dhtDidCopied.value = true
  setTimeout(() => { dhtDidCopied.value = false }, 2000)
}

// DID Document modal
const showDidDocModal = ref(false)
const loadingDidDoc = ref(false)
const didDocumentData = ref<Record<string, unknown> | null>(null)
const didDocVerified = ref<boolean | null>(null)
const didDocCopied = ref(false)

const didDocumentFormatted = computed(() =>
  didDocumentData.value ? JSON.stringify(didDocumentData.value, null, 2) : ''
)

async function showDidDocument() {
  showDidDocModal.value = true
  loadingDidDoc.value = true
  didDocVerified.value = null
  try {
    const doc = await rpcClient.resolveDid()
    didDocumentData.value = doc
    const verification = await rpcClient.call({
      method: 'identity.verify-did-document',
      params: { document: doc },
    }) as { valid: boolean }
    didDocVerified.value = verification.valid
  } catch (err) {
    if (import.meta.env.DEV) console.error('Failed to load DID Document:', err)
    didDocumentData.value = null
  } finally {
    loadingDidDoc.value = false
  }
}

async function copyDidDocument() {
  if (!didDocumentFormatted.value) return
  await safeClipboardWrite(didDocumentFormatted.value)
  didDocCopied.value = true
  setTimeout(() => { didDocCopied.value = false }, 2000)
}

// Hardware wallets
const detectedHwWallets = ref<HwWalletDevice[]>([])

async function detectHardwareWallets() {
  try {
    const res = await rpcClient.detectUsbDevices()
    detectedHwWallets.value = res.devices || []
  } catch {
    detectedHwWallets.value = []
  }
}

// Every-entry: none of these child components use useCachedResource
// internally (confirmed by reading Web5ConnectedNodes.vue/Web5Identities.vue/
// Web5NodeVisibility.vue/Web5NostrRelays.vue — all are plain functions
// exposed via defineExpose), so under KeepAlive these must run again on every
// reactivation or peers/requests/identities/visibility/relays go stale for
// the rest of the session after the first visit.
function armWeb5Live() {
  connectedNodesRef.value?.loadPeers()
  connectedNodesRef.value?.loadReceivedMessages()
  connectedNodesRef.value?.loadConnectionRequests()
  identitiesRef.value?.loadIdentities()
  nodeVisibilityRef.value?.loadVisibility()
  // domainsRef.value?.loadDomainNames()  // hidden for now
  nostrRelaysRef.value?.loadNostrRelays()
  // credentialsRef.value?.loadCredentials()  // hidden for now
  // sharedContentRef.value?.loadContentItems()  // hidden for now

  // Profits resources fetch themselves on first use (and skip the
  // fetch entirely when the cached value is still fresh).
  detectHardwareWallets()

  // Shared content loaded by the component itself via expose
  // The SharedContent component manages its own loadContentItems
}
// Vue fires onActivated immediately after onMounted on a KeepAlive-wrapped
// component's first mount, which would otherwise double every load in
// armWeb5Live() on first mount. onMounted's call counts as the first
// activation's arm; onActivated only re-arms on a genuine later
// reactivation.
let web5FreshMount = true
onActivated(() => {
  if (web5FreshMount) { web5FreshMount = false; return }
  armWeb5Live()
})

onMounted(() => {
  // Once-per-session: the intro-stagger flag and the authoritative DID
  // lookup. Neither needs to re-run on a later reactivation — the DID is
  // stable for the life of the identity and the stagger animation must fire
  // exactly once.
  web5AnimationDone = true

  // Load the authoritative node DID from the backend
  rpcClient.getNodeDid().then(res => {
    if (res.did && res.did !== storedDid.value) {
      storedDid.value = res.did
      try { localStorage.setItem('neode_did', res.did) } catch { /* noop */ }
    }
  }).catch(() => { /* use cached localStorage value */ })

  // Also arms the every-entry loaders directly here: onActivated is a no-op
  // outside a <KeepAlive> boundary, so a bare mount (a unit test, or any
  // future non-KeepAlive usage) must not silently skip them.
  armWeb5Live()
})

</script>
