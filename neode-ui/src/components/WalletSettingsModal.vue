<template>
  <BaseModal :show="show" title="Wallet Settings" max-width="max-w-2xl" content-class="max-h-[90vh]" @close="close">
    <!-- Protocol tabs — pinned via the header slot; only the pane below
         scrolls (2026-07-22 modal contract). -->
    <template #header>
      <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          @click="activeTab = tab.key"
          class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
          :class="activeTab === tab.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
        >{{ tab.label }}</button>
      </div>
    </template>

    <!-- ===================== Lightning Channels ===================== -->
    <div v-show="activeTab === 'channels'">
      <p class="text-white/60 text-sm mb-4">
        Lightning channels on this node. Open a channel to a peer to send and receive Lightning payments.
      </p>
      <LightningChannelsPanel v-if="show" compact />
    </div>

    <!-- ===================== Cashu Mints ===================== -->
    <div v-show="activeTab === 'cashu'">
      <!-- Real vs test ecash. Each network keeps its own wallet, so switching
           parks one balance and reveals the other — it never merges them. -->
      <div class="mb-4 rounded-lg border border-white/10 bg-white/[0.04] p-3">
        <div class="flex items-center justify-between gap-3">
          <div>
            <p class="text-white/80 text-sm font-medium">{{ ecashIsTest ? 'Test ecash' : 'Real ecash' }}</p>
            <p class="text-white/40 text-xs mt-0.5">
              {{ ecashIsTest
                ? 'Practice coins with no value, from a public test mint.'
                : 'Real, spendable sats.' }}
            </p>
          </div>
          <label class="flex items-center gap-2 cursor-pointer shrink-0">
            <span class="text-white/60 text-xs">Test mode</span>
            <input
              type="checkbox"
              :checked="ecashIsTest"
              :disabled="switchingNetwork"
              class="h-4 w-4 accent-orange-500"
              @change="setEcashNetwork(($event.target as HTMLInputElement).checked)"
            />
          </label>
        </div>
        <p v-if="ecashIsTest" class="mt-2 text-xs text-orange-200">
          Your real ecash balance is safe and untouched — it reappears when you turn test mode off.
        </p>
        <p v-if="networkError" class="mt-2 text-xs text-red-300">{{ networkError }}</p>
      </div>

      <p class="text-white/60 text-sm mb-4">
        Cashu ecash tokens can only be received from mints in this list. Add a mint's URL to accept tokens issued by it.
      </p>

      <div v-if="loadingMints" class="py-6 text-center text-white/50 text-sm">Loading mints…</div>

      <template v-else>
        <div class="space-y-2 mb-4">
          <div
            v-for="(mint, idx) in mints"
            :key="mint + idx"
            class="flex items-center justify-between gap-3 p-3 bg-white/5 rounded-lg"
          >
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <svg class="w-5 h-5 text-purple-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span class="text-sm font-mono text-white/90 truncate">{{ mint }}</span>
            </div>
            <button
              @click="removeMint(idx)"
              :disabled="mints.length <= 1"
              class="p-2 rounded-lg hover:bg-white/10 text-white/50 hover:text-red-400 transition-colors disabled:opacity-30 disabled:hover:text-white/50 disabled:hover:bg-transparent shrink-0"
              aria-label="Remove mint"
              title="Remove mint"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
              </svg>
            </button>
          </div>
          <p v-if="mints.length === 0" class="text-white/40 text-sm text-center py-2">No mints configured.</p>
        </div>

        <div class="mb-3">
          <label class="text-white/60 text-sm block mb-1">Add a mint</label>
          <div class="flex gap-2">
            <input
              v-model="newMint"
              type="text"
              placeholder="https://mint.example.com"
              class="flex-1 input-glass font-mono"
              @keydown.enter.prevent="addMint"
            />
            <button @click="addMint" class="glass-button px-4 py-2 rounded-lg text-sm font-medium shrink-0">Add</button>
          </div>
        </div>

        <div v-if="mintError" class="mb-3 alert-error">{{ mintError }}</div>
        <div v-if="mintsSavedOk" class="mb-3 text-xs text-green-400">Accepted mints saved.</div>

      </template>
    </div>

    <!-- ===================== Fedimint Federations ===================== -->
    <div v-show="activeTab === 'fedimint'">
      <div class="flex items-start gap-2 mb-4">
        <p class="text-white/60 text-sm flex-1">
          Join a Fedimint federation by pasting its invite code. Federated ecash is held by a group of guardians rather than a single mint.
        </p>
        <span v-if="!fedimintBackendReady" class="shrink-0 text-[10px] px-2 py-0.5 rounded-full font-medium bg-orange-500/15 text-orange-400">Coming soon</span>
      </div>

      <!-- Joined federations -->
      <div class="space-y-2 mb-4">
        <div
          v-for="fed in federations"
          :key="fed.federation_id"
          class="flex items-center justify-between gap-3 p-3 bg-white/5 rounded-lg"
        >
          <div class="flex items-center gap-3 min-w-0 flex-1">
            <svg class="w-5 h-5 text-blue-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a4 4 0 00-3-3.87M9 20H4v-2a4 4 0 013-3.87m6-.13a4 4 0 10-4-4 4 4 0 004 4zm6 0a4 4 0 10-3-6.65" />
            </svg>
            <div class="min-w-0 flex-1">
              <p class="text-sm text-white/90 truncate">{{ fed.name || fed.federation_id }}</p>
              <p class="text-[11px] text-white/40 font-mono truncate">{{ fed.federation_id }}</p>
            </div>
          </div>
          <span class="text-sm text-blue-400 font-medium shrink-0">{{ fed.balance_sats.toLocaleString() }} sats</span>
        </div>
        <p v-if="federations.length === 0" class="text-white/40 text-sm text-center py-2">No federations joined yet.</p>
      </div>

      <!-- Join by invite code -->
      <div class="mb-3">
        <label class="text-white/60 text-sm block mb-1">Invite code</label>
        <textarea
          v-model="inviteCode"
          rows="3"
          :disabled="!fedimintBackendReady"
          placeholder="fed11jpr3lgm8t…"
          class="w-full input-glass font-mono disabled:opacity-50"
        ></textarea>
      </div>

      <div v-if="fedError" class="mb-3 alert-error">{{ fedError }}</div>
      <div v-if="fedJoinedOk" class="mb-3 text-xs text-green-400">Federation joined.</div>


      <p v-if="!fedimintBackendReady" class="text-[11px] text-white/40 text-center mt-3">
        Joining federations lands with the Fedimint client backend.
      </p>
    </div>

    <!-- ===================== On-chain ===================== -->
    <div v-show="activeTab === 'onchain'">
      <p class="text-white/60 text-sm mb-4">
        Where "view transaction" links open. When the Mempool app is installed and running
        on this node, it is always used — private, no third parties. Without it (pruned
        Bitcoin nodes can't run Mempool), links open on the explorer below.
      </p>

      <label class="block text-sm text-white/80 mb-1">Transaction explorer</label>
      <input
        v-model="explorerUrlInput"
        :placeholder="EXPLORER_PLACEHOLDER"
        spellcheck="false"
        class="w-full rounded-lg bg-white/[0.06] border border-white/10 text-white px-3 py-2 text-sm font-mono focus:outline-none focus:border-orange-400/60"
        @change="saveExplorer"
      />
      <p class="text-[11px] text-white/40 mt-1">
        Any Mempool-compatible instance works. Default: tx1138.com.
      </p>

      <div class="mt-3 p-3 rounded-lg border border-amber-400/25 bg-amber-500/10 text-amber-200/80 text-xs leading-relaxed">
        ⚠️ Opening a transaction on an external explorer tells that server's operator which
        transaction you're interested in, plus your IP address. Use a server you trust.
      </div>

      <label class="flex items-center gap-2 mt-4 text-sm text-white/70 cursor-pointer select-none">
        <input type="checkbox" v-model="explorerAcknowledged" class="accent-orange-400" @change="saveExplorer" />
        Don't warn me each time before opening the external explorer
      </label>

    </div>

    <!-- ===================== Ark ===================== -->
    <div v-show="activeTab === 'ark'">
      <div class="flex items-start gap-2 mb-4">
        <p class="text-white/60 text-sm flex-1">
          Ark holds self-custodial off-chain bitcoin via an Ark server. Funds stay recoverable on-chain even if the server disappears.
        </p>
        <span v-if="arkStatus && !arkStatus.available" class="shrink-0 text-[10px] px-2 py-0.5 rounded-full font-medium bg-orange-500/15 text-orange-400">Not installed</span>
        <span v-else-if="arkStatus?.config?.network && arkStatus.config.network !== 'mainnet'" class="shrink-0 text-[10px] px-2 py-0.5 rounded-full font-medium bg-teal-500/15 text-teal-400">{{ arkStatus.config.network }}</span>
      </div>

      <div v-if="loadingArk" class="py-6 text-center text-white/50 text-sm">Checking Ark wallet…</div>

      <template v-else>
        <p v-if="arkStatus && !arkStatus.available" class="text-white/40 text-sm text-center py-2 mb-4">
          Install the <span class="text-white/70">Ark Wallet</span> app from the app store to enable Ark payments.
        </p>

        <!-- Balances -->
        <div v-if="arkStatus?.available" class="grid grid-cols-3 gap-2 mb-4">
          <div class="p-3 bg-white/5 rounded-lg text-center">
            <p class="text-[11px] text-white/40 mb-1">Spendable</p>
            <p class="text-sm text-teal-400 font-medium">{{ (arkBalance?.spendable_sats ?? 0).toLocaleString() }} sats</p>
          </div>
          <div class="p-3 bg-white/5 rounded-lg text-center">
            <p class="text-[11px] text-white/40 mb-1">Pending</p>
            <p class="text-sm text-white/70 font-medium">{{ (arkBalance?.pending_sats ?? 0).toLocaleString() }} sats</p>
          </div>
          <div class="p-3 bg-white/5 rounded-lg text-center">
            <p class="text-[11px] text-white/40 mb-1">On-chain</p>
            <p class="text-sm text-white/70 font-medium">{{ (arkBalance?.onchain_sats ?? 0).toLocaleString() }} sats</p>
          </div>
        </div>

        <!-- Receive address -->
        <div v-if="arkStatus?.available" class="mb-4">
          <div class="flex gap-2">
            <button @click="fetchArkAddress(false)" :disabled="arkBusy" class="flex-1 glass-button px-3 py-2 rounded-lg text-xs font-medium disabled:opacity-50">Ark address</button>
            <button @click="fetchArkAddress(true)" :disabled="arkBusy" class="flex-1 glass-button px-3 py-2 rounded-lg text-xs font-medium disabled:opacity-50">On-chain (boarding) address</button>
          </div>
          <div v-if="arkAddress" class="mt-2 flex items-center gap-2 p-3 bg-white/5 rounded-lg">
            <span class="text-xs font-mono text-white/90 break-all flex-1">{{ arkAddress }}</span>
            <CopyButton :value="arkAddress" icon-only class="shrink-0" />
          </div>
        </div>

        <!-- Board / offboard -->
        <div v-if="arkStatus?.available" class="flex gap-2 mb-4">
          <button
            @click="boardArk"
            :disabled="arkBusy || (arkBalance?.onchain_sats ?? 0) === 0"
            class="flex-1 glass-button px-3 py-2 rounded-lg text-xs font-medium disabled:opacity-50"
            title="Lift on-chain funds into Ark"
          >{{ arkBoarding ? 'Boarding…' : 'Board on-chain funds' }}</button>
          <button
            @click="offboardArk"
            :disabled="arkBusy || (arkBalance?.spendable_sats ?? 0) === 0"
            class="flex-1 glass-button px-3 py-2 rounded-lg text-xs font-medium disabled:opacity-50"
            title="Move all Ark funds back on-chain"
          >{{ arkOffboarding ? 'Offboarding…' : 'Offboard to on-chain' }}</button>
        </div>

        <!-- Server configuration -->
        <div class="mb-3 space-y-2">
          <label class="text-white/60 text-sm block">Ark server</label>
          <input v-model="arkConfig.ark_server" type="text" placeholder="https://ark.signet.2nd.dev" class="w-full input-glass font-mono" />
          <label class="text-white/60 text-sm block">Esplora (chain source)</label>
          <input v-model="arkConfig.esplora" type="text" placeholder="https://esplora.signet.2nd.dev" class="w-full input-glass font-mono" />
          <label class="text-white/60 text-sm block">Network</label>
          <select v-model="arkConfig.network" class="w-full input-glass">
            <option value="signet">signet</option>
            <option value="mainnet">mainnet</option>
            <option value="regtest">regtest</option>
          </select>
          <p class="text-[11px] text-white/40">
            Applied when the Ark wallet is created — an existing wallet stays bound to its server.
          </p>
        </div>

        <div v-if="arkError" class="mb-3 alert-error">{{ arkError }}</div>
        <div v-if="arkOk" class="mb-3 text-xs text-green-400">{{ arkOk }}</div>

      </template>
    </div>
    <!-- Pinned footer (2026-07-22 modal contract): Close always, plus the
         active tab's primary action — the buttons never scroll away. -->
    <template #footer>
      <div class="flex gap-3">
        <button @click="close" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.close') }}</button>
        <button
          v-if="activeTab === 'cashu'"
          @click="saveMints"
          :disabled="savingMints || mints.length === 0"
          class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
        >{{ savingMints ? 'Saving…' : 'Save' }}</button>
        <button
          v-else-if="activeTab === 'fedimint'"
          @click="joinFederation"
          :disabled="!fedimintBackendReady || joiningFed || !inviteCode.trim()"
          class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
        >{{ joiningFed ? 'Joining…' : 'Join federation' }}</button>
        <button
          v-else-if="activeTab === 'ark' && arkStatus?.available"
          @click="saveArkConfig"
          :disabled="arkBusy || !arkConfig.ark_server.trim() || !arkConfig.esplora.trim()"
          class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
        >{{ savingArk ? 'Saving…' : 'Save' }}</button>
      </div>
    </template>
  </BaseModal>
</template>

<script setup lang="ts">
import CopyButton from '@/components/CopyButton.vue'
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import BaseModal from '@/components/BaseModal.vue'
import LightningChannelsPanel from '@/components/LightningChannelsPanel.vue'
import { useTxExplorer, EXPLORER_PLACEHOLDER } from '@/composables/useTxExplorer'

const { t } = useI18n()

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: []; changed: [] }>()

// Short labels on purpose — five tabs share one row ("Fedi", not
// "Fedimint Federations") so On-chain fits.
const tabs = [
  { key: 'channels' as const, label: 'Channels' },
  { key: 'cashu' as const, label: 'Cashu' },
  { key: 'fedimint' as const, label: 'Fedi' },
  { key: 'ark' as const, label: 'Ark' },
  { key: 'onchain' as const, label: 'On-chain' },
]
const activeTab = ref<'channels' | 'cashu' | 'fedimint' | 'ark' | 'onchain'>('channels')

// Backed by wallet.fedimint-list / -join / -leave (fedimint-clientd HTTP bridge).
// Join degrades gracefully with a clear error if the Fedimint client app isn't installed.
const fedimintBackendReady = true

// ---- On-chain: external transaction explorer ----
const txExplorer = useTxExplorer()
const explorerUrlInput = ref(txExplorer.prefs.value.url)
const explorerAcknowledged = ref(txExplorer.prefs.value.acknowledged)
function saveExplorer() {
  txExplorer.setExplorer(explorerUrlInput.value, explorerAcknowledged.value)
  // Reflect normalization (empty input falls back to the default).
  explorerUrlInput.value = txExplorer.prefs.value.url
}

// ---- Cashu mints ----
const mints = ref<string[]>([])
const newMint = ref('')
const loadingMints = ref(false)
const savingMints = ref(false)
const mintError = ref('')
const mintsSavedOk = ref(false)

// ---- Fedimint federations ----
interface Federation {
  federation_id: string
  name?: string
  balance_sats: number
}
const federations = ref<Federation[]>([])
const inviteCode = ref('')
const joiningFed = ref(false)
const fedError = ref('')
const fedJoinedOk = ref(false)

// ---- Ark (barkd sidecar, wallet.ark-* RPCs) ----
interface ArkStatus {
  available: boolean
  wallet_ready: boolean
  config?: { network: string; ark_server: string; esplora: string }
}
interface ArkBalance {
  spendable_sats: number
  pending_sats: number
  onchain_sats: number
}
const arkStatus = ref<ArkStatus | null>(null)
const arkBalance = ref<ArkBalance | null>(null)
const arkConfig = ref({ network: 'signet', ark_server: '', esplora: '' })
const arkAddress = ref('')
const loadingArk = ref(false)
const savingArk = ref(false)
const arkBoarding = ref(false)
const arkOffboarding = ref(false)
const arkError = ref('')
const arkOk = ref('')
const arkBusy = computed(() => savingArk.value || arkBoarding.value || arkOffboarding.value)

watch(
  () => props.show,
  (open) => {
    if (open) {
      loadMints()
      loadEcashNetwork()
      if (fedimintBackendReady) loadFederations()
      loadArk()
    }
  },
)

// ---- Ecash network (real vs test coins) ----
const ecashIsTest = ref(false)
const switchingNetwork = ref(false)
const networkError = ref('')

async function loadEcashNetwork() {
  try {
    const res = await rpcClient.call<{ is_test: boolean }>({ method: 'wallet.ecash-network' })
    ecashIsTest.value = !!res.is_test
  } catch {
    // Older node without the setting: treat as real ecash, which is the
    // safe reading — never imply a real balance is "test".
    ecashIsTest.value = false
  }
}

async function setEcashNetwork(test: boolean) {
  switchingNetwork.value = true
  networkError.value = ''
  try {
    await rpcClient.call({
      method: 'wallet.ecash-set-network',
      params: { network: test ? 'testnet' : 'mainnet' },
    })
    ecashIsTest.value = test
    // The mint list is per-network, so it must be re-read after a switch.
    await loadMints()
  } catch (err: unknown) {
    networkError.value = err instanceof Error ? err.message : 'Failed to switch network'
    await loadEcashNetwork()
  } finally {
    switchingNetwork.value = false
  }
}

async function loadMints() {
  loadingMints.value = true
  mintError.value = ''
  mintsSavedOk.value = false
  newMint.value = ''
  try {
    const res = await rpcClient.call<{ mints: string[] }>({ method: 'streaming.list-mints' })
    mints.value = res.mints || []
  } catch (err: unknown) {
    mintError.value = err instanceof Error ? err.message : 'Failed to load mints'
    mints.value = []
  } finally {
    loadingMints.value = false
  }
}

function addMint() {
  mintError.value = ''
  mintsSavedOk.value = false
  const url = newMint.value.trim().replace(/\/+$/, '')
  if (!url) return
  if (!url.startsWith('http://') && !url.startsWith('https://')) {
    mintError.value = 'Mint URL must start with http:// or https://'
    return
  }
  if (mints.value.some((m) => m.replace(/\/+$/, '') === url)) {
    mintError.value = 'That mint is already in the list'
    return
  }
  mints.value.push(url)
  newMint.value = ''
}

function removeMint(idx: number) {
  if (mints.value.length <= 1) return
  mints.value.splice(idx, 1)
  mintsSavedOk.value = false
}

async function saveMints() {
  if (mints.value.length === 0) return
  savingMints.value = true
  mintError.value = ''
  mintsSavedOk.value = false
  try {
    await rpcClient.call<{ mints: string[]; updated: boolean }>({
      method: 'streaming.configure-mints',
      params: { mints: mints.value },
    })
    mintsSavedOk.value = true
    emit('changed')
  } catch (err: unknown) {
    mintError.value = err instanceof Error ? err.message : 'Failed to save mints'
  } finally {
    savingMints.value = false
  }
}

async function loadFederations() {
  fedError.value = ''
  try {
    const res = await rpcClient.call<{ federations: Federation[] }>({ method: 'wallet.fedimint-list' })
    federations.value = res.federations || []
  } catch {
    federations.value = []
  }
}

async function joinFederation() {
  if (!fedimintBackendReady || !inviteCode.value.trim()) return
  const before = federations.value.length
  joiningFed.value = true
  fedError.value = ''
  fedJoinedOk.value = false
  try {
    await rpcClient.call<{ federation_id: string }>({
      method: 'wallet.fedimint-join',
      params: { invite_code: inviteCode.value.trim() },
      // Joining a federation is heavy (downloads the federation config + joins
      // the consensus); it routinely takes longer than the default 15s. Give it
      // headroom past the backend's own 60s clientd timeout.
      timeout: 90000,
    })
    inviteCode.value = ''
    await loadFederations()
    fedJoinedOk.value = true
    emit('changed')
  } catch (err: unknown) {
    // A slow join often still completes server-side after the client gives up,
    // so don't cry failure blindly — re-check the list. If a new federation
    // appeared, the join actually worked; surface success instead of a scary
    // (and wrong) timeout error.
    await loadFederations()
    if (federations.value.length > before) {
      inviteCode.value = ''
      fedJoinedOk.value = true
      emit('changed')
    } else {
      fedError.value = err instanceof Error ? err.message : 'Failed to join federation'
    }
  } finally {
    joiningFed.value = false
  }
}

async function loadArk() {
  loadingArk.value = true
  arkError.value = ''
  arkOk.value = ''
  arkAddress.value = ''
  try {
    const status = await rpcClient.call<ArkStatus>({ method: 'wallet.ark-status' })
    arkStatus.value = status
    if (status.config) arkConfig.value = { ...status.config }
    if (status.available) {
      arkBalance.value = await rpcClient.call<ArkBalance>({ method: 'wallet.ark-balance' })
    } else {
      arkBalance.value = null
    }
  } catch {
    arkStatus.value = { available: false, wallet_ready: false }
    arkBalance.value = null
  } finally {
    loadingArk.value = false
  }
}

async function fetchArkAddress(onchain: boolean) {
  arkError.value = ''
  arkOk.value = ''
  try {
    const res = await rpcClient.call<{ address: string }>({
      method: 'wallet.ark-address',
      params: { onchain },
    })
    arkAddress.value = res.address
  } catch (err: unknown) {
    arkError.value = err instanceof Error ? err.message : 'Failed to get address'
  }
}

async function boardArk() {
  arkBoarding.value = true
  arkError.value = ''
  arkOk.value = ''
  try {
    // Boards everything; waits on Ark round participation, so give it room.
    await rpcClient.call({ method: 'wallet.ark-board', timeout: 130000 })
    arkOk.value = 'Boarding started — funds appear as spendable once the round confirms.'
    arkBalance.value = await rpcClient.call<ArkBalance>({ method: 'wallet.ark-balance' })
    emit('changed')
  } catch (err: unknown) {
    arkError.value = err instanceof Error ? err.message : 'Failed to board funds'
  } finally {
    arkBoarding.value = false
  }
}

async function offboardArk() {
  arkOffboarding.value = true
  arkError.value = ''
  arkOk.value = ''
  try {
    await rpcClient.call({ method: 'wallet.ark-offboard', timeout: 130000 })
    arkOk.value = 'Offboard requested — funds return on-chain with the next round.'
    arkBalance.value = await rpcClient.call<ArkBalance>({ method: 'wallet.ark-balance' })
    emit('changed')
  } catch (err: unknown) {
    arkError.value = err instanceof Error ? err.message : 'Failed to offboard funds'
  } finally {
    arkOffboarding.value = false
  }
}

async function saveArkConfig() {
  savingArk.value = true
  arkError.value = ''
  arkOk.value = ''
  try {
    const res = await rpcClient.call<{ config: ArkStatus['config'] }>({
      method: 'wallet.ark-configure',
      params: { ...arkConfig.value },
    })
    if (res.config) arkConfig.value = { ...res.config }
    arkOk.value = 'Ark configuration saved.'
    emit('changed')
  } catch (err: unknown) {
    arkError.value = err instanceof Error ? err.message : 'Failed to save Ark configuration'
  } finally {
    savingArk.value = false
  }
}

function close() {
  mintError.value = ''
  mintsSavedOk.value = false
  fedError.value = ''
  arkError.value = ''
  arkOk.value = ''
  emit('close')
}
</script>
