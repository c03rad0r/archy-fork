<template>
  <!-- Wallet -->
  <div data-controller-container tabindex="0" :class="{ 'card-stagger': showStagger }" class="glass-card p-6 flex flex-col" style="--stagger-index: 1">
    <div class="flex items-start gap-4 mb-4 shrink-0">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z" />
        </svg>
      </div>
      <div class="flex-1 min-w-0">
        <h2 class="text-xl font-semibold text-white mb-2">{{ t('web5.wallet') }}</h2>
        <p class="text-white/70 text-sm mb-4">{{ t('web5.walletSubtitle') }}</p>
      </div>
      <!-- Transaction Activity Badge -->
      <button
        v-if="txActivityCount > 0"
        @click="showIncomingTxPanel = !showIncomingTxPanel"
        class="incoming-tx-badge shrink-0"
      >
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16V4m0 0L3 8m4-4l4 4m6 0v12m0 0l4-4m-4 4l-4-4" />
        </svg>
        <span v-if="incomingTxCount > 0">Incoming {{ incomingTxCount }}</span>
        <span v-if="meshRelayActive" class="ml-1">Mesh TX</span>
        <span class="incoming-tx-ping"></span>
      </button>
    </div>

    <!-- Transaction Activity Panel -->
    <transition name="incoming-tx-slide">
      <div v-if="showIncomingTxPanel && (incomingTransactions.length > 0 || meshRelayActive)" class="mb-4 rounded-xl overflow-hidden border border-green-500/20">
        <div class="px-4 py-2.5 bg-green-500/10 border-b border-green-500/15 flex items-center justify-between">
          <span class="text-xs font-medium text-green-400 uppercase tracking-wide">Transactions</span>
          <button @click="showIncomingTxPanel = false" class="text-white/40 hover:text-white/70 transition-colors">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
          </button>
        </div>
        <div class="divide-y divide-white/5">
          <!-- Mesh Relay TX (outgoing via mesh) -->
          <div v-if="meshRelayActive" class="incoming-tx-row">
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <div class="incoming-tx-icon" style="background: rgba(251,146,60,0.15);">
                <svg class="w-3.5 h-3.5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 10l7-7m0 0l7 7m-7-7v18" />
                </svg>
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-orange-400">Mesh Relay</span>
                  <span class="text-[10px] px-1.5 py-0.5 rounded-full font-medium bg-orange-500/15 text-orange-400">
                    {{ sendResultTxid ? 'Broadcast' : 'Sending...' }}
                  </span>
                </div>
                <p class="text-[11px] text-white/40 mt-0.5">{{ meshRelayStatus }}</p>
              </div>
            </div>
          </div>
          <!-- Incoming TXs -->
          <div
            v-for="tx in incomingTransactions"
            :key="tx.tx_hash"
            class="incoming-tx-row"
            @click="openInMempool(tx.tx_hash)"
          >
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <div class="incoming-tx-icon" :class="tx.num_confirmations === 0 ? 'incoming-tx-icon-pending' : 'incoming-tx-icon-confirmed'">
                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                </svg>
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-green-400">+{{ tx.amount_sats.toLocaleString() }} sats</span>
                  <span
                    class="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
                    :class="tx.num_confirmations === 0 ? 'bg-yellow-500/15 text-yellow-400' : 'bg-green-500/15 text-green-400'"
                  >
                    {{ tx.num_confirmations === 0 ? 'Unconfirmed' : tx.num_confirmations + ' conf' }}
                  </span>
                </div>
                <p class="text-[11px] text-white/40 font-mono truncate mt-0.5">{{ tx.tx_hash }}</p>
              </div>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <span class="text-[11px] text-white/40">{{ formatTxTime(tx.time_stamp) }}</span>
              <svg class="w-3.5 h-3.5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </div>
          </div>
        </div>
      </div>
    </transition>

    <div v-if="walletError" class="alert-error mb-3">{{ walletError }}</div>

    <div class="space-y-3 flex-1 min-h-0">
      <!-- Total Balance -->
      <div class="flex items-center justify-between p-3 bg-white/10 rounded-lg">
        <div class="flex items-center gap-3">
          <span class="text-lg text-orange-500 font-bold">₿</span>
          <span class="text-white text-sm font-medium">{{ t('web5.totalBitcoin') }}</span>
        </div>
        <span class="text-white text-sm font-semibold">{{ (lndOnchainBalance + lndChannelBalance + ecashBalance).toLocaleString() }} sats</span>
      </div>

      <!-- On-chain Balance -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-orange-500" role="img" aria-label="On-chain" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.onChain') }}</span>
        </div>
        <span class="text-orange-500 text-sm font-medium">{{ lndOnchainBalance.toLocaleString() }} sats</span>
      </div>

      <!-- Lightning Balance -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-yellow-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.lightning') }}</span>
        </div>
        <span class="text-yellow-400 text-sm font-medium">{{ lndChannelBalance.toLocaleString() }} sats</span>
      </div>

      <!-- Ecash Balance -->
      <div class="flex items-center justify-between p-3 bg-white/5 rounded-lg">
        <div class="flex items-center gap-3">
          <svg class="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          <span class="text-white/80 text-sm">{{ t('web5.ecash') }}</span>
        </div>
        <span class="text-purple-400 text-sm font-medium">{{ ecashBalance.toLocaleString() }} sats</span>
      </div>
    </div>

    <!-- Action buttons -->
    <div class="grid grid-cols-2 gap-2 mt-auto pt-4 shrink-0">
      <button
        @click="$emit('openSend')"
        :disabled="!walletConnected && ecashBalance <= 0"
        class="px-3 py-2 glass-button rounded-lg text-xs font-medium text-white/90 hover:text-white transition-colors disabled:opacity-50"
      >
        {{ t('common.send') }}
      </button>
      <button
        @click="$emit('openReceive')"
        class="px-3 py-2 glass-button rounded-lg text-xs font-medium text-white/90 hover:text-white transition-colors"
      >
        {{ t('web5.receiveBitcoin') }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatTxTime } from './utils'
import type { WalletTransaction } from './types'
import { useTxExplorer } from '@/composables/useTxExplorer'

const { t } = useI18n()

const showIncomingTxPanel = ref(false)

defineProps<{
  showStagger: boolean
  walletConnected: boolean
  walletError: string
  lndOnchainBalance: number
  lndChannelBalance: number
  ecashBalance: number
  incomingTransactions: WalletTransaction[]
  incomingTxCount: number
  txActivityCount: number
  meshRelayActive: boolean
  meshRelayStatus: string
  sendResultTxid: string
}>()

defineEmits<{
  openSend: []
  openReceive: []
}>()

const txExplorer = useTxExplorer()
function openInMempool(txHash: string) {
  // Local Mempool app when running; external explorer with consent
  // otherwise (pruned nodes can't run Mempool).
  txExplorer.openTx(txHash)
}
</script>
