<template>
  <div>
    <!-- Liquidity Summary -->
    <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6" :class="{ 'md:gap-3 mb-4': compact }">
      <div class="glass-card p-4" :class="{ 'p-3 bg-white/5': compact }">
        <p class="text-white/60 text-sm mb-1">Total Outbound</p>
        <p class="text-white text-xl font-bold" :class="{ 'text-base': compact }">{{ formatSats(summary.total_outbound) }}</p>
      </div>
      <div class="glass-card p-4" :class="{ 'p-3 bg-white/5': compact }">
        <p class="text-white/60 text-sm mb-1">Total Inbound</p>
        <p class="text-white text-xl font-bold" :class="{ 'text-base': compact }">{{ formatSats(summary.total_inbound) }}</p>
      </div>
      <div class="glass-card p-4" :class="{ 'p-3 bg-white/5': compact }">
        <p class="text-white/60 text-sm mb-1">Channels</p>
        <p class="text-white text-xl font-bold" :class="{ 'text-base': compact }">{{ channels.length }}</p>
      </div>
    </div>

    <!-- Zeus channel suggestion -->
    <div class="glass-card p-4 mb-4 border border-orange-500/25">
      <div class="flex flex-col sm:flex-row sm:items-center gap-4">
        <img
          src="/assets/img/app-icons/zeus.webp"
          alt="Zeus"
          class="w-12 h-12 rounded-xl shrink-0 border border-white/10"
        />
        <div class="flex-1 min-w-0">
          <p class="text-white/90 text-sm font-semibold mb-0.5">Open a channel with Zeus</p>
          <p class="text-white/55 text-xs leading-relaxed">
            Open a channel with Zeus Olympus node and start sending and receiving Lightning
            payments. Minimum 150,000 · maximum 1,500,000 on-chain sats required.
          </p>
        </div>
        <div class="flex sm:flex-col items-center gap-2 shrink-0">
          <button
            @click="openZeusChannel"
            class="glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium whitespace-nowrap"
          >
            Open Channel
          </button>
          <a
            href="https://zeusln.com"
            target="_blank"
            rel="noopener noreferrer"
            class="text-xs text-orange-400/80 hover:text-orange-300 whitespace-nowrap"
          >Get Zeus →</a>
        </div>
      </div>
    </div>

    <!-- Open Channel Button -->
    <div class="flex justify-end mb-4">
      <button @click="showOpenModal = true" class="glass-button px-4 py-2 rounded-lg text-sm font-medium flex items-center gap-2">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
        </svg>
        Open Channel
      </button>
    </div>

    <Transition name="content-fade" mode="out-in">
    <!-- Loading -->
    <div v-if="loading && channels.length === 0" key="loading" class="glass-card p-12 text-center">
      <svg class="animate-spin h-8 w-8 text-blue-400 mx-auto mb-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
      <p class="text-white/70">Loading channels...</p>
    </div>

    <!-- Error -->
    <div v-else-if="error && channels.length === 0" key="error" class="glass-card p-6 text-center">
      <p class="text-red-300 mb-4">{{ error }}</p>
      <button @click="loadChannels" class="glass-button px-4 py-2 rounded-lg text-sm">Retry</button>
    </div>

    <!-- No Channels -->
    <div v-else-if="channels.length === 0 && closedChannels.length === 0" key="empty" class="glass-card p-8 text-center">
      <svg class="w-16 h-16 text-white/20 mx-auto mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
      </svg>
      <p class="text-white/70 mb-2">No channels yet</p>
      <p class="text-white/50 text-sm">Open a channel to start sending and receiving Lightning payments.</p>
    </div>

    <!-- Channel List -->
    <div v-else key="channels" class="space-y-3">
      <!-- Status tabs -->
      <div class="flex gap-1 p-1 bg-white/5 rounded-lg">
        <button
          v-for="tab in tabs"
          :key="tab.key"
          @click="activeTab = tab.key"
          class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors flex items-center justify-center gap-1.5"
          :class="activeTab === tab.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
        >
          {{ tab.label }}
          <span
            class="px-1.5 py-0.5 rounded-full text-[10px] leading-none"
            :class="activeTab === tab.key ? 'bg-white/15 text-white/80' : 'bg-white/10 text-white/40'"
          >{{ tab.count }}</span>
        </button>
      </div>

      <div v-if="loading" class="p-2 text-center text-white/45 text-xs flex items-center justify-center gap-2">
        <svg class="animate-spin h-3.5 w-3.5" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
      </svg>
        Refreshing channels...
      </div>
      <div v-else-if="error" class="p-3 rounded-lg border border-red-400/20 bg-red-500/10 text-red-200/85 text-sm">
        {{ error }}
      </div>
      <div
        v-for="ch in filteredChannels"
        :key="ch.chan_id || ch.channel_point"
        class="glass-card p-4"
        :class="{ 'bg-white/5': compact }"
      >
        <div class="flex items-center justify-between mb-3">
          <div class="flex items-center gap-2">
            <span
              class="w-2 h-2 rounded-full"
              :class="{
                'bg-green-400': channelStatus(ch) === 'active',
                'bg-yellow-400': channelStatus(ch) === 'pending_open',
                'bg-red-400': channelStatus(ch) === 'inactive',
                'bg-gray-400': channelStatus(ch) === 'closing',
                'bg-gray-500': channelStatus(ch) === 'force_closing',
              }"
            ></span>
            <span class="text-white/80 text-sm font-medium capitalize">{{ channelStatus(ch).replace('_', ' ') }}</span>
          </div>
          <button
            v-if="!['pending_open', 'closing', 'force_closing'].includes(channelStatus(ch))"
            @click="confirmClose(ch)"
            class="text-red-400/70 hover:text-red-400 text-xs transition-colors"
          >
            Close
          </button>
        </div>

        <!-- Peer -->
        <p class="text-white/50 text-xs font-mono mb-3 truncate" :title="ch.remote_pubkey">
          {{ ch.remote_pubkey }}
        </p>

        <!-- Capacity Bar -->
        <div class="mb-2">
          <div class="flex justify-between text-xs text-white/60 mb-1">
            <span>Local: {{ formatSats(ch.local_balance) }}</span>
            <span>Remote: {{ formatSats(ch.remote_balance) }}</span>
          </div>
          <div class="h-2 bg-white/10 rounded-full overflow-hidden flex">
            <div
              class="bg-blue-400 h-full transition-all"
              :style="{ width: capacityPercent(ch.local_balance, ch.capacity) + '%' }"
            ></div>
            <div
              class="bg-orange-400 h-full transition-all"
              :style="{ width: capacityPercent(ch.remote_balance, ch.capacity) + '%' }"
            ></div>
          </div>
          <p class="text-white/40 text-xs mt-1 text-center">
            Capacity: {{ formatSats(ch.capacity) }}
          </p>
        </div>

        <!-- Funding / closing tx -->
        <div v-if="fundingTxid(ch) || ch.closing_txid" class="flex justify-end gap-3">
          <button
            v-if="fundingTxid(ch)"
            @click="openInMempool(fundingTxid(ch))"
            class="flex items-center gap-1 text-blue-400/70 hover:text-blue-400 text-xs transition-colors"
            :title="fundingTxid(ch)"
          >
            Funding tx in Mempool
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </button>
          <button
            v-if="ch.closing_txid"
            @click="openInMempool(ch.closing_txid!)"
            class="flex items-center gap-1 text-orange-400/70 hover:text-orange-400 text-xs transition-colors"
            :title="ch.closing_txid"
          >
            Closing tx in Mempool
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Closed channel history (All + Closed tabs) -->
      <div
        v-for="ch in filteredClosed"
        :key="'closed-' + (ch.chan_id || ch.channel_point || ch.closing_tx_hash)"
        class="glass-card p-4 opacity-75"
        :class="{ 'bg-white/5': compact }"
      >
        <div class="flex items-center justify-between mb-3">
          <div class="flex items-center gap-2">
            <span class="w-2 h-2 rounded-full bg-white/30"></span>
            <span class="text-white/60 text-sm font-medium">Closed</span>
            <span v-if="closeTypeLabel(ch)" class="text-white/40 text-xs">· {{ closeTypeLabel(ch) }}</span>
          </div>
          <span v-if="ch.close_height" class="text-white/35 text-xs">Block {{ ch.close_height.toLocaleString() }}</span>
        </div>

        <p class="text-white/40 text-xs font-mono mb-3 truncate" :title="ch.remote_pubkey">
          {{ ch.remote_pubkey }}
        </p>

        <div class="flex justify-between text-xs text-white/50 mb-2">
          <span>Settled: {{ formatSats(ch.settled_balance) }}</span>
          <span>Capacity: {{ formatSats(ch.capacity) }}</span>
        </div>

        <div v-if="ch.closing_tx_hash" class="flex justify-end">
          <button
            @click="openInMempool(ch.closing_tx_hash)"
            class="flex items-center gap-1 text-blue-400/70 hover:text-blue-400 text-xs transition-colors"
            :title="ch.closing_tx_hash"
          >
            Closing tx in Mempool
            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Per-tab empty state -->
      <div
        v-if="filteredChannels.length === 0 && filteredClosed.length === 0"
        class="glass-card p-6 text-center"
      >
        <p class="text-white/50 text-sm">{{ emptyTabMessage }}</p>
      </div>
    </div>
    </Transition>

    <!-- Open Channel Modal -->
    <Teleport to="body">
    <div v-if="showOpenModal" class="fixed inset-0 z-[3100] flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="showOpenModal = false">
      <div class="glass-card p-6 w-full max-w-md mx-4 max-h-[90vh] overflow-y-auto">
        <h2 class="text-lg font-bold text-white mb-4">Open Channel</h2>

        <div class="space-y-4">
          <div>
            <label class="text-white/60 text-sm block mb-1">Peer URI</label>
            <input
              v-model="openForm.peerUri"
              type="text"
              placeholder="pubkey@host:port"
              class="w-full input-glass"
            />
            <p class="text-white/40 text-xs mt-1">Format: pubkey@host:port</p>
          </div>
          <div>
            <label class="text-white/60 text-sm block mb-1">Amount (sats)</label>
            <input
              v-model.number="openForm.amount"
              type="number"
              min="20000"
              placeholder="100000"
              class="w-full input-glass"
            />
            <p class="text-white/40 text-xs mt-1">Minimum 20,000 sats</p>
          </div>

          <!-- Fee selection -->
          <div>
            <label class="text-white/60 text-sm block mb-1">Fee</label>
            <div class="flex gap-1 p-1 bg-white/5 rounded-lg">
              <button
                v-for="preset in feePresets"
                :key="preset.key"
                @click="openForm.feePreset = preset.key"
                class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
                :class="openForm.feePreset === preset.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
              >{{ preset.label }}</button>
            </div>
            <p v-if="openForm.feePreset !== 'custom'" class="text-white/40 text-xs mt-1">
              {{ feePresets.find(p => p.key === openForm.feePreset)?.hint }}
            </p>
            <div v-else class="grid grid-cols-2 gap-3 mt-2">
              <div>
                <label class="text-white/60 text-xs block mb-1">Target confirmations</label>
                <input
                  v-model.number="openForm.customConfTarget"
                  type="number"
                  min="1"
                  max="1008"
                  placeholder="6"
                  class="w-full input-glass"
                />
              </div>
              <div>
                <label class="text-white/60 text-xs block mb-1">Sats per vByte</label>
                <input
                  v-model.number="openForm.customSatPerVbyte"
                  type="number"
                  min="1"
                  max="5000"
                  placeholder="—"
                  class="w-full input-glass"
                />
              </div>
              <p class="text-white/40 text-xs col-span-2">Set one — sats per vByte takes precedence when both are set</p>
            </div>
          </div>

          <div>
            <label class="flex items-center gap-2 cursor-pointer">
              <input v-model="openForm.private" type="checkbox" class="accent-blue-500" />
              <span class="text-white/60 text-sm">Private channel (unannounced)</span>
            </label>
            <p class="text-white/40 text-xs mt-1">Some nodes (e.g. LSPs like Olympus) only accept unannounced channels</p>
          </div>
        </div>

        <!-- "Still starting" is a wait-a-moment notice, not a failure — show
             it as calm amber info instead of the red error treatment. -->
        <div
          v-if="openError"
          class="mt-3"
          :class="isStartupNotice(openError)
            ? 'p-3 rounded-lg border border-amber-400/25 bg-amber-500/10 text-amber-200/90'
            : 'alert-error'"
        >
          <p class="text-xs">
            <span v-if="isStartupNotice(openError)" class="mr-1">⏳</span>{{ openError }}
          </p>
        </div>

        <div class="flex gap-3 mt-6">
          <button @click="showOpenModal = false" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
          <button
            @click="openChannel"
            :disabled="openingChannel"
            class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium"
          >
            {{ openingChannel ? 'Opening...' : 'Open Channel' }}
          </button>
        </div>
      </div>
    </div>
    </Teleport>

    <!-- Close Confirmation Modal -->
    <Teleport to="body">
    <div v-if="closeTarget" class="fixed inset-0 z-[3100] flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="!closingChannel && (closeTarget = null)">
      <div class="glass-card p-6 w-full max-w-sm mx-4">
        <h2 class="text-lg font-bold text-white mb-2">Close Channel?</h2>
        <p class="text-white/60 text-sm mb-4">This will cooperatively close the channel with peer {{ closeTarget.remote_pubkey.slice(0, 16) }}...</p>
          <!-- Fee selection -->
          <div class="mb-4">
            <label class="text-white/60 text-sm block mb-1">Fee</label>
            <div class="flex gap-1 p-1 bg-white/5 rounded-lg">
              <button
                v-for="preset in feePresets"
                :key="preset.key"
                @click="closeForm.feePreset = preset.key"
                class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
                :class="closeForm.feePreset === preset.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
              >{{ preset.label }}</button>
            </div>
            <p v-if="closeForm.feePreset !== 'custom'" class="text-white/40 text-xs mt-1">
              {{ feePresets.find(p => p.key === closeForm.feePreset)?.hint }}
            </p>
            <div v-else class="grid grid-cols-2 gap-3 mt-2">
              <div>
                <label class="text-white/60 text-xs block mb-1">Target confirmations</label>
                <input
                  v-model.number="closeForm.customConfTarget"
                  type="number"
                  min="1"
                  max="1008"
                  placeholder="6"
                  class="w-full input-glass"
                />
              </div>
              <div>
                <label class="text-white/60 text-xs block mb-1">Sats per vByte</label>
                <input
                  v-model.number="closeForm.customSatPerVbyte"
                  type="number"
                  min="1"
                  max="5000"
                  placeholder="—"
                  class="w-full input-glass"
                />
              </div>
              <p class="text-white/40 text-xs col-span-2">Set one — sats per vByte takes precedence when both are set</p>
            </div>
          </div>

        <div v-if="closeError" class="mb-3 alert-error">
          <p class="text-xs">{{ closeError }}</p>
        </div>
        <div class="flex gap-3">
          <button @click="closeTarget = null" :disabled="closingChannel" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Cancel</button>
          <button
            @click="closeChannel"
            :disabled="closingChannel"
            class="flex-1 glass-button glass-button-danger px-4 py-2 rounded-lg text-sm font-medium"
          >
            {{ closingChannel ? 'Closing...' : 'Close' }}
          </button>
        </div>
      </div>
    </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import { useTxExplorer } from '@/composables/useTxExplorer'

defineProps<{ compact?: boolean }>()

interface Channel {
  chan_id: string
  remote_pubkey: string
  capacity: number
  local_balance: number
  remote_balance: number
  active: boolean
  status?: string
  channel_point?: string
  closing_txid?: string
}

interface ClosedChannel {
  chan_id?: string
  remote_pubkey: string
  capacity: number
  settled_balance: number
  close_type?: string
  closing_tx_hash?: string
  channel_point?: string
  close_height?: number
}

/** Status with a fallback derived from `active` for backends that omit it */
function channelStatus(ch: Channel): string {
  return ch.status ?? (ch.active ? 'active' : 'inactive')
}

type ChannelTab = 'all' | 'active' | 'pending' | 'closed'
const activeTab = ref<ChannelTab>('all')

/** pending_open, closing and force_closing all live on the Pending tab */
function isPendingState(ch: Channel): boolean {
  return ['pending_open', 'closing', 'force_closing'].includes(channelStatus(ch))
}

const tabs = computed((): { key: ChannelTab; label: string; count: number }[] => [
  { key: 'all', label: 'All', count: channels.value.length + closedChannels.value.length },
  { key: 'active', label: 'Active', count: channels.value.filter(ch => !isPendingState(ch)).length },
  { key: 'pending', label: 'Pending', count: channels.value.filter(isPendingState).length },
  { key: 'closed', label: 'Closed', count: closedChannels.value.length },
])

const filteredChannels = computed((): Channel[] => {
  switch (activeTab.value) {
    case 'closed': return []
    case 'active': return channels.value.filter(ch => !isPendingState(ch))
    case 'pending': return channels.value.filter(isPendingState)
    default: return channels.value
  }
})

const filteredClosed = computed((): ClosedChannel[] =>
  activeTab.value === 'all' || activeTab.value === 'closed' ? closedChannels.value : []
)

const emptyTabMessage = computed((): string => {
  switch (activeTab.value) {
    case 'active': return 'No open channels.'
    case 'pending': return 'No pending or closing channels.'
    case 'closed': return 'No closed channels yet.'
    default: return 'No channels yet.'
  }
})

/** "COOPERATIVE_CLOSE" / "cooperative_close" → "cooperative close" */
function closeTypeLabel(ch: ClosedChannel): string {
  return (ch.close_type || '').toLowerCase().replace(/_/g, ' ')
}

type FeePreset = 'standard' | 'medium' | 'fast' | 'custom'

const feePresets: { key: FeePreset; label: string; hint?: string; confTarget?: number }[] = [
  { key: 'standard', label: 'Standard', hint: 'Targets ~6 blocks (about an hour)', confTarget: 6 },
  { key: 'medium', label: 'Medium', hint: 'Targets ~3 blocks (about 30 minutes)', confTarget: 3 },
  { key: 'fast', label: 'Fast', hint: 'Targets the next block', confTarget: 1 },
  { key: 'custom', label: 'Custom' },
]

// Cached: revisits paint the channel lists instantly and revalidate behind
// them. Open and closed history are separate entries so a closed-history
// failure keeps its last list without touching the main channel view.
interface ChannelsData { channels: Channel[]; total_inbound: number; total_outbound: number }
const channelsRes = useCachedResource<ChannelsData>({
  key: 'lnd.channels',
  fetcher: async (signal) => {
    const result = await rpcClient.call<ChannelsData>({
      method: 'lnd.listchannels', timeout: 15000, signal, dedup: true, maxRetries: 1,
    })
    return {
      channels: result?.channels || [],
      total_inbound: result?.total_inbound || 0,
      total_outbound: result?.total_outbound || 0,
    }
  },
  persist: false, // open channel balances/capacity — wallet data (T-02-01)
})
const closedRes = useCachedResource<ClosedChannel[]>({
  key: 'lnd.closed-channels',
  fetcher: async (signal) => {
    const closed = await rpcClient.call<{ channels: ClosedChannel[] }>({
      method: 'lnd.closedchannels', timeout: 15000, signal, dedup: true, maxRetries: 1,
    })
    return closed?.channels || []
  },
  persist: false, // closed channel settlement records — wallet data (T-02-01)
})
const loading = computed(() =>
  channelsRes.loadState.value === 'loading' || channelsRes.loadState.value === 'refreshing')
const error = computed(() => channelsRes.error.value)
const channels = computed(() => channelsRes.data.value?.channels ?? [])
const closedChannels = computed(() => closedRes.data.value ?? [])
const summary = computed(() => ({
  total_inbound: channelsRes.data.value?.total_inbound ?? 0,
  total_outbound: channelsRes.data.value?.total_outbound ?? 0,
}))

// Olympus by ZEUS — the LSP node behind the Zeus mobile wallet.
// Channel limits: min 150,000 / max 1,500,000 sats.
const OLYMPUS_PEER_URI =
  '031b301307574bbe9b9ac7b79cbe1700e31e544513eae0b5d7497483083f99e581@45.79.192.236:9735'

const showOpenModal = ref(false)
const defaultOpenForm = () => ({
  peerUri: '',
  amount: 100000,
  private: false,
  feePreset: 'fast' as FeePreset,
  customConfTarget: null as number | null,
  customSatPerVbyte: null as number | null,
})

/** Prefill the open-channel modal for a Zeus (Olympus) channel */
function openZeusChannel() {
  openForm.value = {
    ...defaultOpenForm(),
    peerUri: OLYMPUS_PEER_URI,
    amount: 150000,
    // Olympus only accepts unannounced channels
    private: true,
  }
  openError.value = null
  showOpenModal.value = true
}
const openForm = ref(defaultOpenForm())
const openingChannel = ref(false)
const openError = ref<string | null>(null)

/** LND's transient post-unlock state ("still finishing its startup") is a
 *  wait-a-moment notice, not a failure — the template styles it amber. */
function isStartupNotice(msg: string | null): boolean {
  if (!msg) return false
  const m = msg.toLowerCase()
  return m.includes('still finishing its startup') || m.includes('in the process of starting')
}

const closeTarget = ref<Channel | null>(null)
const closingChannel = ref(false)
const closeError = ref<string | null>(null)

function formatSats(sats: number): string {
  if (sats >= 100_000_000) return `${(sats / 100_000_000).toFixed(2)} BTC`
  if (sats >= 1_000_000) return `${(sats / 1_000_000).toFixed(1)}M sats`
  if (sats >= 1_000) return `${(sats / 1_000).toFixed(1)}k sats`
  return `${sats} sats`
}

function fundingTxid(ch: Channel): string {
  const txid = ch.channel_point?.split(':')[0] || ''
  return /^[0-9a-fA-F]{64}$/.test(txid) ? txid : ''
}

const txExplorer = useTxExplorer()
function openInMempool(txid: string) {
  if (!txid) return
  // Same routing as every other tx link (missed in the first pass —
  // 2026-07-22): local Mempool app when it's running, otherwise the saved
  // external explorer, with the first-time consent modal setting it up and
  // then opening the tx.
  txExplorer.openTx(txid)
}

function capacityPercent(amount: number, capacity: number): number {
  if (capacity <= 0) return 0
  return Math.round((amount / capacity) * 100)
}

function loadChannels(): Promise<void> {
  const main = channelsRes.refresh()
  void closedRes.refresh()
  return main
}

function feeParams(
  form: { feePreset: FeePreset; customSatPerVbyte: number | null; customConfTarget: number | null } = openForm.value,
  setError: (message: string) => void = message => { openError.value = message },
): { target_conf?: number; sat_per_vbyte?: number } | null {
  if (form.feePreset !== 'custom') {
    return { target_conf: feePresets.find(p => p.key === form.feePreset)?.confTarget ?? 1 }
  }
  const rate = form.customSatPerVbyte
  const conf = form.customConfTarget
  if (rate != null && rate !== 0) {
    if (!Number.isInteger(rate) || rate < 1 || rate > 5000) { setError('Sats per vByte must be a whole number between 1 and 5000'); return null }
    return { sat_per_vbyte: Math.floor(rate) }
  }
  if (conf != null && conf !== 0) {
    if (!Number.isInteger(conf) || conf < 1 || conf > 1008) { setError('Target confirmations must be a whole number between 1 and 1008'); return null }
    return { target_conf: Math.floor(conf) }
  }
  setError('Custom fee requires target confirmations or sats per vByte')
  return null
}

async function openChannel() {
  if (openingChannel.value) return
  openError.value = null

  const uri = openForm.value.peerUri.trim()
  if (!uri) { openError.value = 'Peer URI is required'; return }
  if (openForm.value.amount < 20000) { openError.value = 'Minimum 20,000 sats'; return }

  const fee = feeParams()
  if (!fee) return

  const parts = uri.split('@')
  const pubkey = parts[0]
  const address = parts[1] || undefined

  openingChannel.value = true
  try {
    await rpcClient.call({
      method: 'lnd.openchannel',
      params: { pubkey, address, amount: openForm.value.amount, private: openForm.value.private, ...fee },
      // Server may wait up to 35s for a synchronous peer connect before opening
      timeout: 60000,
    })
    showOpenModal.value = false
    openForm.value = defaultOpenForm()
    await loadChannels()
  } catch (err: unknown) {
    openError.value = err instanceof Error ? err.message : 'Failed to open channel'
  } finally {
    openingChannel.value = false
  }
}

const defaultCloseForm = () => ({ feePreset: 'fast' as FeePreset, customConfTarget: null as number | null, customSatPerVbyte: null as number | null })
const closeForm = ref(defaultCloseForm())

function confirmClose(ch: Channel) {
  if (closingChannel.value) return
  closeForm.value = defaultCloseForm()
  closeTarget.value = ch
  closeError.value = null
}

async function closeChannel() {
  if (closingChannel.value || !closeTarget.value) return
  closeError.value = null
  const fee = feeParams(closeForm.value, message => { closeError.value = message })
  if (!fee) return
  closingChannel.value = true
  try {
    await rpcClient.call({
      method: 'lnd.closechannel',
      params: { channel_point: closeTarget.value.channel_point, ...fee },
      timeout: 45000,
      maxRetries: 1,
    })
    closeTarget.value = null
    await loadChannels()
  } catch (err: unknown) {
    closeError.value = err instanceof Error ? err.message : 'Failed to close channel'
  } finally {
    closingChannel.value = false
  }
}

defineExpose({ channels, loadChannels })
</script>
