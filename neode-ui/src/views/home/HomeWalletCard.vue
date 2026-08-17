<template>
  <div
    data-controller-container
    tabindex="0"
    class="home-card controller-focusable"
    :class="{ 'home-card-animate': animate }"
    style="--card-stagger: 4"
  >
    <div class="home-card-shell">
      <div class="home-card-inner p-6 flex flex-col h-full min-h-0">
        <div class="home-card-header flex items-start justify-between mb-4 shrink-0">
          <div class="home-card-text flex items-start gap-4">
            <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
              <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z" />
              </svg>
            </div>
            <div>
              <h2 class="text-xl font-semibold text-white mb-1">{{ t('web5.wallet') }}</h2>
              <p class="text-sm text-white/70">{{ walletConnected ? t('common.connected') : t('common.disconnected') }}</p>
            </div>
          </div>
          <div class="flex items-center gap-2">
            <button
              @click="incomingTxCount > 0 || showIncomingTxPanel ? toggleIncomingPanel() : $emit('showTransactions')"
              :class="incomingTxCount > 0 ? 'incoming-tx-badge' : 'text-white/50 hover:text-white/80 text-xs px-2 py-1 rounded-lg bg-white/5 hover:bg-white/10 transition-colors'"
              class="shrink-0"
            >
              <template v-if="incomingTxCount > 0">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                </svg>
                <span>Incoming {{ incomingTxCount }}</span>
                <span class="incoming-tx-ping"></span>
              </template>
              <template v-else>
                <svg class="w-3.5 h-3.5 inline mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
                </svg>
                <span>Transactions</span>
              </template>
            </button>
            <button
              @click="$emit('showWalletSettings')"
              aria-label="Wallet settings"
              title="Wallet settings"
              class="text-white/60 hover:text-white transition-colors"
            >
              <svg class="w-5 h-5" aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
          </div>
        </div>

        <!-- Incoming Transactions Panel -->
        <transition name="incoming-tx-slide">
          <div v-if="showIncomingTxPanel && incomingTransactions.length > 0" class="mb-4 rounded-xl overflow-hidden border border-green-500/20">
            <div class="px-4 py-2.5 bg-green-500/10 border-b border-green-500/15 flex items-center justify-between">
              <span class="text-xs font-medium text-green-400 uppercase tracking-wide">Incoming Transactions</span>
              <button @click="toggleIncomingPanel" class="text-white/40 hover:text-white/70 transition-colors">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
              </button>
            </div>
            <div class="divide-y divide-white/5">
              <div
                v-for="tx in incomingTransactions"
                :key="txKey(tx)"
                class="flex items-center justify-between gap-3 px-4 py-3 hover:bg-white/5 transition-colors"
                :class="isOnchain(tx) ? 'cursor-pointer' : ''"
                @click="isOnchain(tx) && $emit('openInMempool', tx.tx_hash)"
              >
                <div class="flex items-center gap-3 min-w-0 flex-1">
                  <div
                    class="w-7 h-7 rounded-full flex items-center justify-center shrink-0"
                    :class="isOnchain(tx) && tx.num_confirmations === 0 ? 'bg-yellow-500/15' : 'bg-green-500/15'"
                  >
                    <svg class="w-3.5 h-3.5" :class="isOnchain(tx) && tx.num_confirmations === 0 ? 'text-yellow-400' : 'text-green-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                    </svg>
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium text-green-400">+{{ tx.amount_sats.toLocaleString() }} sats</span>
                      <span
                        v-if="isOnchain(tx)"
                        class="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
                        :class="tx.num_confirmations === 0 ? 'bg-yellow-500/15 text-yellow-400' : 'bg-green-500/15 text-green-400'"
                      >
                        {{ tx.num_confirmations === 0 ? 'Unconfirmed' : tx.num_confirmations + ' conf' }}
                      </span>
                      <span
                        v-else
                        class="text-[10px] px-1.5 py-0.5 rounded-full font-medium bg-green-500/15 text-green-400"
                      >
                        {{ railBadge(tx) }}
                      </span>
                    </div>
                    <p class="text-[11px] text-white/40 font-mono truncate mt-0.5">{{ tx.tx_hash }}</p>
                  </div>
                </div>
                <div class="flex items-center gap-2 shrink-0">
                  <span class="text-[11px] text-white/40">{{ formatTxTime(tx.time_stamp) }}</span>
                  <svg v-if="isOnchain(tx)" class="w-3.5 h-3.5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                  </svg>
                </div>
              </div>
            </div>
          </div>
        </transition>

        <div class="home-card-stats space-y-3 mb-4 flex-1 min-h-0">
          <div class="flex items-center justify-between p-3 bg-white/10 rounded-lg">
            <div class="flex items-center gap-3">
              <span class="text-lg text-orange-500 font-bold">&#x20bf;</span>
              <span class="text-sm font-medium text-white">{{ t('web5.totalBitcoin') }}</span>
            </div>
            <span class="text-white text-sm font-semibold">{{ walletTotal.toLocaleString() }} sats</span>
          </div>
          <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
            <div class="flex items-center gap-3">
              <svg class="w-5 h-5 text-orange-500" role="img" aria-label="On-chain" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
              </svg>
              <span class="text-sm text-white/80">{{ t('web5.onChain') }}</span>
            </div>
            <span class="text-orange-500 text-sm font-medium">{{ walletOnchain.toLocaleString() }} sats</span>
          </div>
          <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
            <div class="flex items-center gap-3">
              <svg class="w-5 h-5 text-yellow-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
              </svg>
              <span class="text-sm text-white/80">{{ t('web5.lightning') }}</span>
            </div>
            <span class="text-yellow-400 text-sm font-medium">{{ walletLightning.toLocaleString() }} sats</span>
          </div>
          <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
            <div class="flex items-center gap-3">
              <!-- Inline SVG, not an emoji: kiosk images ship no color-emoji
                   font, so 🥜 rendered as a tofu box on TVs -->
              <svg class="w-5 h-5 text-purple-400" role="img" aria-label="Cashu" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z" />
              </svg>
              <span class="text-sm text-white/80">Cashu</span>
            </div>
            <span class="text-purple-400 text-sm font-medium">{{ walletEcash.toLocaleString() }} sats</span>
          </div>
          <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
            <div class="flex items-center gap-3">
              <svg class="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a4 4 0 00-3-3.87M9 20H4v-2a4 4 0 013-3.87m6-.13a4 4 0 10-4-4 4 4 0 004 4zm6 0a4 4 0 10-3-6.65" />
              </svg>
              <span class="text-sm text-white/80">Fedimint</span>
            </div>
            <span class="text-blue-400 text-sm font-medium">{{ walletFedimint.toLocaleString() }} sats</span>
          </div>
          <!-- Only rendered once barkd reports a balance — most nodes don't run the Ark sidecar -->
          <div v-if="(walletArk ?? 0) > 0" class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
            <div class="flex items-center gap-3">
              <svg class="w-5 h-5 text-teal-400" role="img" aria-label="Ark" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8a2.5 2.5 0 10-2.5-2.5A2.5 2.5 0 0012 8zm0 0v13m-7-7c0 3.87 3.13 7 7 7s7-3.13 7-7m-14 0H3m18 0h-2" />
              </svg>
              <span class="text-sm text-white/80">Ark</span>
            </div>
            <span class="text-teal-400 text-sm font-medium">{{ (walletArk ?? 0).toLocaleString() }} sats</span>
          </div>
        </div>
        <div class="home-card-buttons flex gap-2 mt-auto pt-4 shrink-0">
          <button @click="$emit('showSend')" class="home-card-btn flex-1 px-3 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">
            {{ t('common.send') }}
          </button>
          <button @click="$emit('showReceive')" class="home-card-btn flex-1 px-3 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">
            Receive
          </button>
          <button @click="$emit('showTransactions')" class="home-card-btn flex-1 px-3 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors">
            Transactions
          </button>
          <button v-if="isDev" @click="$emit('faucet')" class="home-card-btn px-3 py-2 glass-button rounded-lg text-sm font-medium text-center transition-colors text-green-400">
            Faucet
          </button>
          <button
            @click="$emit('showScan')"
            aria-label="Scan QR to send"
            title="Scan QR to send"
            class="home-card-btn px-3 py-2 glass-button rounded-lg transition-colors shrink-0 flex items-center justify-center"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8V6a2 2 0 012-2h2M3 16v2a2 2 0 002 2h2m10-16h2a2 2 0 012 2v2m-4 12h2a2 2 0 002-2v-2M7 12h10" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

export interface WalletTransaction {
  tx_hash: string
  amount_sats: number
  direction: 'incoming' | 'outgoing'
  num_confirmations: number
  time_stamp: number
  total_fees: number
  dest_addresses: string[]
  label: string
  block_height: number
  // Which rail the transaction happened on; absent = onchain (older backends)
  kind?: 'onchain' | 'lightning' | 'cashu' | 'fedimint' | 'ark'
}

const props = defineProps<{
  animate: boolean
  walletConnected: boolean
  walletOnchain: number
  walletLightning: number
  walletEcash: number
  walletFedimint: number
  walletArk?: number
  walletTransactions: WalletTransaction[]
  isDev: boolean
}>()

defineEmits<{
  showScan: []
  showSend: []
  showReceive: []
  showTransactions: []
  showWalletSettings: []
  faucet: []
  openInMempool: [txHash: string]
}>()

const showIncomingTxPanel = ref(false)

const walletTotal = computed(() =>
  props.walletOnchain + props.walletLightning + props.walletEcash + props.walletFedimint + (props.walletArk ?? 0)
)

function isOnchain(tx: WalletTransaction): boolean {
  return !tx.kind || tx.kind === 'onchain'
}

// Instant rails (lightning/cashu/fedimint/ark) settle immediately, so there is
// no confirmation to wait for and no natural moment for them to leave the
// badge. They used to drop out on a five-minute wall clock, which meant a
// receipt could appear and then silently disappear before anyone looked at it
// — and if you opened the panel a few minutes late, the payment you came to
// check on had already evaporated. For ecash that is the worst case
// available: it leaves no public ledger entry, so this panel was the only
// place the receipt was ever shown.
//
// So they stay until they have actually been *seen*. Opening the panel is
// what marks them seen, which is the same unread model the mesh inbox uses.
// On-chain is unchanged — a confirmation count is a real signal and does the
// job by itself.
const seenIncoming = ref(new Set<string>())

function txKey(tx: WalletTransaction): string {
  // Instant rails have no txid to key on, so fall back to rail+time+amount.
  return tx.tx_hash || `${tx.kind ?? 'onchain'}:${tx.time_stamp}:${tx.amount_sats}`
}

const incomingTransactions = computed(() =>
  props.walletTransactions.filter(tx => {
    if (tx.direction !== 'incoming') return false
    if (isOnchain(tx)) return tx.num_confirmations < 3
    return !seenIncoming.value.has(txKey(tx))
  })
)
const incomingTxCount = computed(() => incomingTransactions.value.length)

/// Open or close the panel. Closing is what acknowledges the instant-rail
/// receipts currently listed — marking them on *open* would make a row vanish
/// under the cursor of someone still reading it.
function toggleIncomingPanel() {
  if (showIncomingTxPanel.value) {
    for (const tx of incomingTransactions.value) {
      if (!isOnchain(tx)) seenIncoming.value.add(txKey(tx))
    }
    // Vue tracks Set mutations, but reassigning keeps the dependency obvious.
    seenIncoming.value = new Set(seenIncoming.value)
    showIncomingTxPanel.value = false
    return
  }
  showIncomingTxPanel.value = true
}

function railBadge(tx: WalletTransaction): string {
  if (tx.kind === 'lightning') return '⚡ Instant'
  if (tx.kind === 'cashu') return 'Cashu'
  if (tx.kind === 'fedimint') return 'Fedimint'
  if (tx.kind === 'ark') return 'Ark'
  return ''
}

function formatTxTime(timestamp: number): string {
  if (!timestamp) return ''
  const date = new Date(timestamp * 1000)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMin = Math.floor(diffMs / 60000)
  if (diffMin < 1) return 'Just now'
  if (diffMin < 60) return `${diffMin}m ago`
  const diffHours = Math.floor(diffMin / 60)
  if (diffHours < 24) return `${diffHours}h ago`
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return `${diffDays}d ago`
  return date.toLocaleDateString()
}
</script>
