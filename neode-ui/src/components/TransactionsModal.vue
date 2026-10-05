<template>
  <!-- Mobile: cap at ~60% of the LIVE visible viewport (not dvh — see
       syncViewportHeightVar in main.ts) so the tx list doesn't fill the screen. -->
  <BaseModal :show="show" :title="t('transactions.title')" max-width="max-w-2xl" content-class="max-h-[calc(var(--visual-viewport-height,100dvh)*0.6)] md:max-h-[90vh] flex flex-col" @close="close">
        <!-- Rail filter: instant ecash micro-payments pile up fast and bury
             on-chain/Lightning rows; chips keep the standard txs reachable.
             Sticky inside the modal's scroll region so the tabs stay pinned
             while the transactions scroll. Only the buttons have a visual
             surface; the rail has no fill, blur, border or shadow. -->
        <div v-if="transactions.length > 0" class="sticky top-0 z-10 -mx-2 px-2 pb-2 mb-1 flex gap-1.5 flex-nowrap overflow-x-auto bg-transparent">
          <button
            v-for="f in filters"
            :key="f.key"
            class="shrink-0 whitespace-nowrap px-2.5 py-1 rounded-full text-xs transition-colors"
            :class="activeFilter === f.key
              ? 'bg-orange-500/25 text-orange-200 border border-orange-400/40'
              : 'bg-white/5 text-white/50 border border-white/10 hover:text-white/80'"
            @click="activeFilter = f.key"
          >
            {{ f.label }}<span v-if="countFor(f.key)" class="text-white/35"> · {{ countFor(f.key) }}</span>
          </button>
        </div>

        <div v-if="transactions.length === 0" class="flex items-center justify-center py-12">
          <p class="text-white/40 text-sm">{{ t('transactions.noTransactionsYet') }}</p>
        </div>

        <div v-else-if="filteredTransactions.length === 0" class="flex items-center justify-center py-12">
          <p class="text-white/40 text-sm">No {{ activeFilter }} transactions</p>
        </div>

        <!-- Must NOT be its own scroll container: BaseModal's slot wrapper is the
             scroller, and modal-scroll-locked gives every .overflow-y-auto inside
             the overlay overscroll-behavior:contain — a nested scroller with no
             overflow then swallows touch scrolling entirely (phones only; wheel
             latches onto the scrollable ancestor and never sees the bug). -->
        <div v-else class="-mx-2 px-2 divide-y divide-white/5">
          <div
            v-for="tx in filteredTransactions"
            :key="(tx.kind || 'onchain') + tx.tx_hash + tx.time_stamp"
            class="flex items-center justify-between gap-3 py-3 hover:bg-white/5 rounded-lg px-2 transition-colors"
            :class="isOnchain(tx) ? 'cursor-pointer' : 'cursor-default'"
            @click="openInMempool(tx)"
          >
            <div class="flex items-center gap-3 min-w-0 flex-1">
              <div
                class="w-8 h-8 rounded-full flex items-center justify-center shrink-0"
                :class="tx.direction === 'incoming'
                  ? (tx.num_confirmations === 0 ? 'bg-yellow-500/15' : 'bg-green-500/15')
                  : 'bg-red-500/10'"
              >
                <svg
                  class="w-4 h-4"
                  :class="tx.direction === 'incoming'
                    ? (tx.num_confirmations === 0 ? 'text-yellow-400' : 'text-green-400')
                    : 'text-red-400'"
                  fill="none" stroke="currentColor" viewBox="0 0 24 24"
                >
                  <path v-if="tx.direction === 'incoming'" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                  <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 10l7-7m0 0l7 7m-7-7v18" />
                </svg>
              </div>
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                  <span
                    class="text-sm font-medium whitespace-nowrap"
                    :class="tx.direction === 'incoming' ? 'text-green-400' : 'text-red-400'"
                  >
                    {{ tx.direction === 'incoming' ? '+' : '-' }}{{ displayAmount(tx).toLocaleString() }} sats
                  </span>
                  <span
                    v-if="isOnchain(tx)"
                    class="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
                    :class="tx.num_confirmations === 0
                      ? 'bg-yellow-500/15 text-yellow-400'
                      : tx.num_confirmations < 3
                        ? 'bg-green-500/15 text-green-400'
                        : 'bg-white/10 text-white/50'"
                  >
                    {{ tx.num_confirmations === 0 ? t('transactions.unconfirmed') : t('transactions.confirmations', { count: tx.num_confirmations }) }}
                  </span>
                  <span
                    v-else
                    class="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
                    :class="tx.kind === 'lightning' ? 'bg-yellow-500/15 text-yellow-400' : tx.kind === 'cashu' ? 'bg-purple-500/15 text-purple-400' : tx.kind === 'ark' ? 'bg-teal-500/15 text-teal-400' : 'bg-blue-500/15 text-blue-400'"
                  >
                    {{ kindLabel(tx) }}
                  </span>
                </div>
                <div class="flex items-center gap-2 mt-0.5">
                  <p class="text-[11px] text-white/40 font-mono truncate">{{ tx.tx_hash }}</p>
                  <span v-if="feeFor(tx)" class="text-[10px] text-white/35 shrink-0">fee {{ feeFor(tx).toLocaleString() }} sats</span>
                  <span v-if="tx.label" class="text-[10px] text-white/30 shrink-0">{{ tx.label }}</span>
                </div>
                <details v-if="tx.fee_bump_history?.length" class="mt-1 text-[11px] text-white/50" @click.stop>
                  <summary class="cursor-pointer">Fee history</summary>
                  <p>Original fee {{ tx.total_fees.toLocaleString() }} sats</p>
                  <button v-for="bump in tx.fee_bump_history" :key="bump.tx_hash" type="button"
                    class="block max-w-full truncate text-left hover:text-white/80"
                    @click.stop="txExplorer.openTx(bump.tx_hash)">
                    {{ bump.status === 'replaced' ? 'Replaced bump' : 'Fee bump' }} · {{ bump.fee_sats.toLocaleString() }} sats · {{ bump.status }} · {{ bump.tx_hash }}
                  </button>
                </details>
              </div>
            </div>
            <div class="flex items-center gap-2 shrink-0">
              <button v-if="isOnchain(tx) && tx.direction === 'outgoing' && (tx.num_confirmations === 0 || (tx.fee_bump_txid && tx.fee_bump_confirmations === 0))" type="button"
                class="px-2 py-1 rounded-md border border-orange-400/25 text-orange-200 text-[11px] hover:bg-orange-500/15"
                :aria-label="`Bump transaction ${tx.tx_hash}`" @click.stop="bumpTxid = tx.fee_bump_txid || tx.tx_hash">Bump</button>
              <span class="hidden sm:inline text-[11px] text-white/40">{{ formatTxTime(tx.time_stamp) }}</span>
              <svg v-if="isOnchain(tx)" class="w-3.5 h-3.5 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </div>
          </div>
        </div>
  </BaseModal>
  <BumpFeeModal :show="show && !!bumpTxid" :txid="bumpTxid" @close="bumpTxid = ''" @updated="emit('updated')" />
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import BaseModal from '@/components/BaseModal.vue'
import BumpFeeModal from '@/components/BumpFeeModal.vue'
import { useTxExplorer } from '@/composables/useTxExplorer'

interface WalletTransaction {
  tx_hash: string
  amount_sats: number
  direction: 'incoming' | 'outgoing'
  num_confirmations: number
  time_stamp: number
  total_fees: number
  bump_fee_sats?: number
  fee_bump_txid?: string
  fee_bump_confirmations?: number
  fee_bump_history?: Array<{ tx_hash: string; fee_sats: number; status: string }>
  dest_addresses: string[]
  label: string
  block_height: number
  // Which rail the transaction happened on; absent = onchain (older backends)
  kind?: 'onchain' | 'lightning' | 'cashu' | 'fedimint' | 'ark'
}

const props = defineProps<{
  show: boolean
  transactions: WalletTransaction[]
}>()

const emit = defineEmits<{ close: []; updated: [] }>()
const bumpTxid = ref('')
const { t } = useI18n()

type FilterKey = 'all' | 'onchain' | 'lightning' | 'ecash' | 'ark'
// The Ark chip only appears once an Ark transaction exists — most nodes
// don't run the barkd sidecar.
const filters = computed<Array<{ key: FilterKey; label: string }>>(() => [
  { key: 'all', label: 'All' },
  { key: 'onchain', label: 'On-chain' },
  { key: 'lightning', label: '⚡ Lightning' },
  { key: 'ecash', label: 'Ecash' },
  ...(props.transactions.some(tx => tx.kind === 'ark') ? [{ key: 'ark' as const, label: 'Ark' }] : []),
])
const activeFilter = ref<FilterKey>('all')

function matchesFilter(tx: WalletTransaction, f: FilterKey): boolean {
  if (f === 'all') return true
  if (f === 'onchain') return isOnchain(tx)
  if (f === 'lightning') return tx.kind === 'lightning'
  if (f === 'ark') return tx.kind === 'ark'
  return tx.kind === 'cashu' || tx.kind === 'fedimint'
}

const filteredTransactions = computed(() => props.transactions.filter(tx => matchesFilter(tx, activeFilter.value)))
function countFor(f: FilterKey): number {
  if (f === 'all') return 0
  return props.transactions.filter(tx => matchesFilter(tx, f)).length
}

function close() {
  bumpTxid.value = ''
  emit('close')
}

// Only on-chain transactions exist in mempool; Lightning/ecash rows don't link
function isOnchain(tx: WalletTransaction): boolean {
  return !tx.kind || tx.kind === 'onchain'
}

function feeFor(tx: WalletTransaction): number {
  return tx.direction === 'outgoing' ? (tx.total_fees || 0) + (tx.bump_fee_sats || 0) : 0
}

/** Outgoing rows show the amount the RECIPIENT got (gross minus fee); the fee
 *  itself is broken out on its own tag. Incoming rows are untouched. */
function displayAmount(tx: WalletTransaction): number {
  const gross = Math.abs(tx.amount_sats)
  const fee = tx.direction === 'outgoing' ? (tx.total_fees || 0) : 0
  return fee > 0 && gross > fee ? gross - fee : gross
}

function kindLabel(tx: WalletTransaction): string {
  if (tx.kind === 'lightning') return '⚡ Lightning'
  if (tx.kind === 'cashu') return 'Cashu'
  if (tx.kind === 'fedimint') return 'Fedimint'
  if (tx.kind === 'ark') return 'Ark'
  return ''
}

const txExplorer = useTxExplorer()
function openInMempool(tx: WalletTransaction) {
  if (!isOnchain(tx)) return
  // Local Mempool app when running (overlaid above this modal); external
  // explorer with consent otherwise (pruned nodes can't run Mempool).
  txExplorer.openTx(tx.tx_hash)
}

function formatTxTime(timestamp: number): string {
  if (!timestamp) return ''
  const date = new Date(timestamp * 1000)
  const now = new Date()
  const diffMs = now.getTime() - date.getTime()
  const diffMins = Math.floor(diffMs / 60000)
  if (diffMins < 1) return t('transactions.justNow')
  if (diffMins < 60) return t('transactions.minutesAgo', { count: diffMins })
  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return t('transactions.hoursAgo', { count: diffHours })
  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return t('transactions.daysAgo', { count: diffDays })
  return date.toLocaleDateString()
}
</script>
