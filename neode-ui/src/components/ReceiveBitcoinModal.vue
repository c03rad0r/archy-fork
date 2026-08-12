<template>
  <BaseModal :show="show" :title="t('web5.receiveBitcoinTitle')" max-width="max-w-2xl" content-class="max-h-[90vh] overflow-y-auto" @close="close">
        <!-- Method tabs -->
        <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
          <button
            v-for="m in (['onchain', 'lightning', 'ecash', 'ark'] as const)"
            :key="m"
            @click="receiveMethod = m"
            class="flex-1 px-2 py-1.5 rounded text-xs font-medium capitalize transition-colors"
            :class="receiveMethod === m ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
          >{{ m === 'onchain' ? t('receiveBitcoin.onChain') : m === 'lightning' ? t('receiveBitcoin.lightning') : m === 'ecash' ? t('receiveBitcoin.ecash') : 'Ark' }}</button>
        </div>

        <!-- Lightning -->
        <div v-if="receiveMethod === 'lightning'">
          <div class="mb-3">
            <label class="text-white/60 text-sm block mb-1">{{ t('receiveBitcoin.amountSats') }}</label>
            <input v-model.number="invoiceAmount" type="number" min="1" placeholder="1000" class="w-full input-glass" />
          </div>
          <div class="mb-3">
            <label class="text-white/60 text-sm block mb-1">{{ t('receiveBitcoin.memoOptional') }}</label>
            <input v-model="invoiceMemo" type="text" :placeholder="t('receiveBitcoin.memoPlaceholder')" class="w-full input-glass" />
          </div>
          <div v-if="invoiceResult" class="mb-3 p-3 bg-white/5 rounded-lg text-center">
            <canvas ref="lightningQrCanvas" class="mx-auto mb-3 rounded-lg" style="image-rendering: pixelated;"></canvas>
            <p class="text-white/50 text-xs mb-1">{{ t('receiveBitcoin.invoiceShareLabel') }}</p>
            <p class="text-xs font-mono text-white/80 break-all">{{ invoiceResult }}</p>
            <CopyButton :value="invoiceResult" :label="t('common.copy')" class="mt-2" />
          </div>
        </div>

        <!-- On-chain -->
        <div v-if="receiveMethod === 'onchain'">
          <!-- Payment detected: the QR did its job — show the outcome -->
          <div v-if="paymentSeen" class="mb-3 p-6 bg-white/5 rounded-lg text-center">
            <div class="flex justify-center mb-4">
              <div
                class="w-16 h-16 rounded-full flex items-center justify-center"
                :class="paymentSeen.confirmations > 0 ? 'bg-green-500/15' : 'bg-orange-500/15 animate-pulse'"
              >
                <!-- Check once confirmed, clock while in the mempool -->
                <svg v-if="paymentSeen.confirmations > 0" class="w-8 h-8 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
                <svg v-else class="w-8 h-8 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
              </div>
            </div>
            <p class="text-lg font-semibold text-white mb-1">
              {{ paymentSeen.confirmations > 0 ? t('receiveBitcoin.paymentConfirmed') : t('receiveBitcoin.paymentBroadcast') }}
            </p>
            <p v-if="paymentSeen.amountSats > 0" class="text-2xl font-semibold text-white/95 mb-2">
              {{ paymentSeen.amountSats.toLocaleString() }} sats
            </p>
            <p v-if="paymentSeen.confirmations === 0" class="text-sm text-white/50 mb-3 max-w-md mx-auto">
              {{ t('receiveBitcoin.paymentBroadcastHint') }}
            </p>
            <p class="text-xs text-white/50 mb-1">{{ t('receiveBitcoin.transactionId') }}</p>
            <p class="text-xs font-mono text-white/80" :title="paymentSeen.txid">{{ midTxid(paymentSeen.txid) }}</p>
            <CopyButton :value="paymentSeen.txid" :label="t('common.copy')" class="mt-2" />
          </div>
          <template v-else>
            <div v-if="note" class="mb-3 p-3 rounded-lg bg-orange-500/10 border border-orange-500/20 text-sm text-white/80 leading-relaxed">
              {{ note }}
            </div>
            <div v-if="onchainAddress" class="mb-3 p-3 bg-white/5 rounded-lg text-center">
              <canvas ref="onchainQrCanvas" class="mx-auto mb-3 rounded-lg" style="image-rendering: pixelated;"></canvas>
              <p class="text-white/50 text-xs mb-2">{{ t('receiveBitcoin.yourBitcoinAddress') }}</p>
              <p class="text-sm font-mono text-white/90 break-all">{{ onchainAddress }}</p>
              <CopyButton :value="onchainAddress" :label="t('common.copy')" class="mt-2" />
            </div>
            <div v-else class="mb-3 text-center">
              <p class="text-white/50 text-sm mb-2">{{ t('web5.generateFreshAddress') }}</p>
              <p v-if="processing" class="text-xs text-white/40">Checking Lightning wallet readiness...</p>
            </div>
          </template>
        </div>

        <!-- Ark -->
        <div v-if="receiveMethod === 'ark'">
          <div v-if="arkAddress" class="mb-3 p-3 bg-white/5 rounded-lg text-center">
            <canvas ref="arkQrCanvas" class="mx-auto mb-3 rounded-lg" style="image-rendering: pixelated;"></canvas>
            <p class="text-white/50 text-xs mb-2">Your Ark address</p>
            <p class="text-sm font-mono text-white/90 break-all">{{ arkAddress }}</p>
            <CopyButton :value="arkAddress" :label="t('common.copy')" class="mt-2" />
          </div>
          <div v-else class="mb-3 text-center">
            <p class="text-white/50 text-sm mb-2">Generate a fresh Ark address to receive off-chain sats instantly.</p>
          </div>
        </div>

        <!-- Ecash -->
        <div v-if="receiveMethod === 'ecash'">
          <div class="mb-3">
            <label class="text-white/60 text-sm block mb-1">{{ t('receiveBitcoin.pasteEcashToken') }}</label>
            <textarea v-model="ecashToken" rows="3" placeholder="cashuB… (Cashu) or Fedimint notes" class="w-full input-glass font-mono"></textarea>
          </div>
          <div v-if="ecashResult" class="mb-3 text-xs text-green-400">{{ ecashResult }}</div>
        </div>

        <div v-if="error" class="mb-3 alert-error">{{ error }}</div>

        <!-- Once the payment is seen there is nothing left to do here -->
        <div v-if="paymentSeen" class="flex">
          <button @click="close" class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium">{{ t('common.done') }}</button>
        </div>
        <div v-else class="flex gap-3">
          <button @click="close" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.close') }}</button>
          <button @click="$emit('scan')" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm font-medium flex items-center justify-center gap-2">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8V6a2 2 0 012-2h2M3 16v2a2 2 0 002 2h2m10-16h2a2 2 0 012 2v2m-4 12h2a2 2 0 002-2v-2M7 12h10" />
            </svg>
            Scan
          </button>
          <button @click="receive" :disabled="processing" class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
            {{ processing ? t('receiveBitcoin.processing') : receiveMethod === 'onchain' ? t('receiveBitcoin.generateAddress') : receiveMethod === 'lightning' ? t('receiveBitcoin.createInvoice') : receiveMethod === 'ark' ? 'Get Ark address' : t('receiveBitcoin.receive') }}
          </button>
        </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, nextTick, watch, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import BaseModal from '@/components/BaseModal.vue'
import CopyButton from '@/components/CopyButton.vue'
import { explainReceiveAddressFailure } from '@/utils/bitcoinReceive'
import { useLightningRequired } from '@/composables/useLightningRequired'

const { t } = useI18n()
const lightning = useLightningRequired()

const props = defineProps<{
  show: boolean
  /** Optional info banner shown on the on-chain tab (e.g. Zeus channel limits) */
  note?: string
  /** Generate an on-chain address immediately when the modal opens */
  autoGenerate?: boolean
}>()
const emit = defineEmits<{ close: []; received: []; scan: [] }>()

watch(() => props.show, (open) => {
  if (!open) {
    stopWatchingPayment()
    return
  }
  paymentSeen.value = null
  // Blank slate on every open: a leftover amount/memo/token or a previous
  // invoice quietly carrying into a new receive flow is exactly the stale-
  // state class the operator flagged on the send modal (2026-08-05).
  receiveMethod.value = 'onchain'
  invoiceAmount.value = 0
  invoiceMemo.value = ''
  invoiceResult.value = ''
  onchainAddress.value = ''
  arkAddress.value = ''
  ecashToken.value = ''
  ecashResult.value = ''
  error.value = ''
  processing.value = false
  if (props.autoGenerate && receiveMethod.value === 'onchain') {
    void receive()
  }
})

const receiveMethod = ref<'lightning' | 'onchain' | 'ecash' | 'ark'>('onchain')
const invoiceAmount = ref<number>(0)
const invoiceMemo = ref('')
const invoiceResult = ref('')
const onchainAddress = ref('')
const arkAddress = ref('')
const ecashToken = ref('')
const ecashResult = ref('')
const onchainQrCanvas = ref<HTMLCanvasElement | null>(null)
const lightningQrCanvas = ref<HTMLCanvasElement | null>(null)
const arkQrCanvas = ref<HTMLCanvasElement | null>(null)
const processing = ref(false)
const error = ref('')

// ── On-chain payment detection ────────────────────────────────────────────
// The generated address is FRESH (lnd.newaddress), so any incoming wallet
// transaction paying it is this receive — no baseline bookkeeping needed.
// Poll while the QR is showing; flip to the success view on first sight
// (0-conf, clock), keep polling gently until the first confirmation
// upgrades it to a check, then stop.
const paymentSeen = ref<null | { txid: string; amountSats: number; confirmations: number }>(null)
let watchTimer: ReturnType<typeof setInterval> | null = null

function midTxid(txid: string): string {
  return txid.length > 24 ? `${txid.slice(0, 10)}…${txid.slice(-10)}` : txid
}

function stopWatchingPayment() {
  if (watchTimer) {
    clearInterval(watchTimer)
    watchTimer = null
  }
}

function startWatchingPayment() {
  stopWatchingPayment()
  watchTimer = setInterval(() => void checkForPayment(), 5000)
}

async function checkForPayment() {
  if (!props.show || !onchainAddress.value) {
    stopWatchingPayment()
    return
  }
  try {
    const res = await rpcClient.call<{
      transactions: Array<{
        tx_hash: string
        amount: number
        num_confirmations: number
        dest_addresses: string[]
        direction: string
      }>
    }>({ method: 'lnd.gettransactions' })
    const hit = (res.transactions || []).find(
      (tx) => tx.direction === 'incoming' && (tx.dest_addresses || []).includes(onchainAddress.value),
    )
    if (!hit) return
    const firstSighting = !paymentSeen.value
    paymentSeen.value = {
      txid: hit.tx_hash,
      amountSats: hit.amount,
      confirmations: hit.num_confirmations,
    }
    if (firstSighting) emit('received')
    if (hit.num_confirmations > 0) stopWatchingPayment()
  } catch {
    // Transient poll failure (daemon busy, LND mid-restart) — keep watching.
  }
}

onUnmounted(stopWatchingPayment)

async function renderQr(data: string, canvas: HTMLCanvasElement | null, prefix = '') {
  if (!canvas || !data) return
  try {
    const QRCode = await import('qrcode')
    await QRCode.toCanvas(canvas, prefix ? `${prefix}${data}` : data, {
      width: 200,
      margin: 2,
      color: { dark: '#000000', light: '#ffffff' },
    })
  } catch { /* QR rendering failed silently */ }
}

function close() {
  stopWatchingPayment()
  paymentSeen.value = null
  invoiceResult.value = ''
  onchainAddress.value = ''
  arkAddress.value = ''
  ecashToken.value = ''
  ecashResult.value = ''
  error.value = ''
  emit('close')
}

async function receive() {
  processing.value = true
  error.value = ''
  try {
    if (receiveMethod.value === 'lightning') {
      // No Lightning implementation installed is not an error — it is a
      // missing prerequisite. Raise the install modal instead of letting
      // lnd.createinvoice fail with connection-refused (FED-08 follow-up).
      if (!(await lightning.requireLightningReady('receive'))) return
      if (!invoiceAmount.value) { error.value = t('receiveBitcoin.enterAnAmount'); return }
      const res = await rpcClient.call<{ payment_request: string }>({
        method: 'lnd.createinvoice',
        params: { amount_sats: invoiceAmount.value, memo: invoiceMemo.value || undefined },
      })
      invoiceResult.value = res.payment_request
      nextTick(() => renderQr(res.payment_request, lightningQrCanvas.value, 'lightning:'))
    } else if (receiveMethod.value === 'onchain') {
      const res = await rpcClient.call<{ address: string }>({ method: 'lnd.newaddress' })
      if (!res.address) {
        throw new Error('LND did not return a Bitcoin address')
      }
      onchainAddress.value = res.address
      paymentSeen.value = null
      startWatchingPayment()
      nextTick(() => renderQr(res.address, onchainQrCanvas.value, 'bitcoin:'))
    } else if (receiveMethod.value === 'ark') {
      const res = await rpcClient.call<{ address: string }>({ method: 'wallet.ark-address' })
      if (!res.address) throw new Error('barkd did not return an Ark address')
      arkAddress.value = res.address
      nextTick(() => renderQr(res.address, arkQrCanvas.value))
    } else {
      if (!ecashToken.value.trim()) { error.value = t('receiveBitcoin.pasteAnEcashToken'); return }
      // The backend auto-detects the token type: a Cashu token (cashuA/B…) is
      // redeemed at its mint, anything else is reissued as Fedimint notes.
      const res = await rpcClient.call<{ received_sats?: number; kind?: string }>({
        method: 'wallet.ecash-receive',
        params: { token: ecashToken.value.trim() },
      })
      const kind = res.kind === 'fedimint' ? 'Fedimint' : 'Cashu'
      ecashResult.value = res.received_sats != null
        ? `Received ${res.received_sats.toLocaleString()} sats (${kind})!`
        : t('receiveBitcoin.tokenReceivedSuccess')
      emit('received')
    }
  } catch (err: unknown) {
    // A running node with no inbound liquidity is a funding problem, not a
    // failure — reuse the Lightning modal in its funding mode instead.
    if (receiveMethod.value === 'lightning' && lightning.handleLightningFailure(err)) return
    error.value = receiveMethod.value === 'onchain'
      ? explainReceiveAddressFailure(err)
      : err instanceof Error ? err.message : 'Failed'
  } finally {
    processing.value = false
  }
}
</script>
