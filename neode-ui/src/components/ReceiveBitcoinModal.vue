<template>
  <BaseModal :show="show && !receiveSuccess" :title="t('web5.receiveBitcoinTitle')" max-width="max-w-2xl" content-class="max-h-[90vh] overflow-y-auto" @close="close">
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
            <p class="text-white/50 text-xs mb-2">{{ t('receiveBitcoin.invoiceShareLabel') }}</p>
            <CopyButton :value="invoiceResult" :label="t('common.copy')" />
          </div>
        </div>

        <!-- On-chain -->
        <div v-if="receiveMethod === 'onchain'">
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
          <!-- Shareable @minibits.cash Lightning address (LUD-16): any Lightning
               wallet can pay this node by address, and the sats land as ecash.
               Fetched on tab open; claimed payments are polled in while open. -->
          <div v-if="lnAddress" class="mb-4 p-3 bg-white/5 rounded-lg text-center">
            <p class="text-white/60 text-sm mb-2">{{ t('receiveBitcoin.lnAddressTitle') }}</p>
            <canvas ref="lnAddressQrCanvas" class="mx-auto mb-3 rounded-lg" style="image-rendering: pixelated;"></canvas>
            <p class="text-white/50 text-xs mb-1">{{ t('receiveBitcoin.lnAddressLabel') }}</p>
            <p class="text-base font-mono text-white/95 break-all mb-2">{{ lnAddress }}</p>
            <CopyButton :value="lnAddress" :label="t('common.copy')" />
            <p class="text-white/40 text-xs mt-3 leading-relaxed">{{ t('receiveBitcoin.lnAddressHint') }}</p>
            <p v-if="lnPendingClaims > 0" class="text-orange-400 text-sm mt-2">
              {{ t('receiveBitcoin.lnAddressPendingRetry', { count: lnPendingClaims }) }}
            </p>
          </div>
          <div v-else-if="lnAddressLoading" class="mb-4 text-center text-white/50 text-sm py-4">
            {{ t('receiveBitcoin.lnAddressLoading') }}
          </div>
          <div v-else-if="lnAddressNeedsSetup" class="mb-3">
            <p class="text-sm text-white/70 mb-3">Set up this wallet's recovery phrase once to enable its Lightning address.</p>
            <EcashSeedBackup setup-only @ready="loadLnAddress" />
          </div>
          <div v-else-if="lnAddressError" class="mb-3 text-xs text-white/40">
            {{ t('receiveBitcoin.lnAddressUnavailable') }}
            <button type="button" class="glass-button rounded-lg px-3 py-2 ml-2" @click="loadLnAddress">Retry</button>
          </div>

          <div class="mb-3">
            <label class="text-white/60 text-sm block mb-1">{{ t('receiveBitcoin.pasteEcashToken') }}</label>
            <textarea v-model="ecashToken" rows="3" placeholder="cashuB… (Cashu) or Fedimint notes" class="w-full input-glass font-mono"></textarea>
          </div>
        </div>

        <div v-if="error" class="mb-3 alert-error">{{ error }}</div>

        <div class="flex gap-3">
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

  <!-- Completion is deliberately its own modal, matching the Lightning
       payment moment. It is not an inline status inside the receive form. -->
  <BaseModal
    :show="show && !!receiveSuccess"
    title="Payment received"
    max-width="max-w-2xl"
    content-class="max-h-[90vh] overflow-y-auto"
    @close="close"
  >
    <PaymentSuccessPane
      v-if="receiveSuccess"
      :amount="receiveSuccess.amount"
      :verb="receiveSuccess.verb"
      :method-label="receiveSuccess.methodLabel"
      :rows="receiveSuccess.rows"
      :note="receiveSuccess.note"
      again-label="Receive another"
      @again="receiveAnother"
      @done="close"
    />
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, nextTick, watch, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import BaseModal from '@/components/BaseModal.vue'
import CopyButton from '@/components/CopyButton.vue'
import EcashSeedBackup from '@/components/EcashSeedBackup.vue'
import PaymentSuccessPane, { type SuccessRow } from '@/components/PaymentSuccessPane.vue'
import { explainReceiveAddressFailure } from '@/utils/bitcoinReceive'
import { useLightningRequired } from '@/composables/useLightningRequired'

const { t } = useI18n()
const lightning = useLightningRequired()

const props = defineProps<{
  show: boolean
  initialMethod?: 'lightning' | 'onchain' | 'ecash' | 'ark'
  /** Optional info banner shown on the on-chain tab (e.g. Zeus channel limits) */
  note?: string
  /** Generate an on-chain address immediately when the modal opens */
  autoGenerate?: boolean
}>()
const emit = defineEmits<{ close: []; received: []; scan: [] }>()

watch(() => props.show, (open) => {
  if (!open) {
    stopWatchingPayment()
    stopWatchingInvoice()
    stopLnClaimPoll()
    return
  }
  paymentSeen.value = null
  invoicePaid.value = null
  invoiceRHash.value = ''
  stopWatchingInvoice()
  // Blank slate on every open: a leftover amount/memo/token or a previous
  // invoice quietly carrying into a new receive flow is exactly the stale-
  // state class the operator flagged on the send modal (2026-08-05).
  receiveMethod.value = props.initialMethod ?? 'onchain'
  invoiceAmount.value = 0
  invoiceMemo.value = ''
  invoiceResult.value = ''
  onchainAddress.value = ''
  arkAddress.value = ''
  ecashToken.value = ''
  receiveSuccess.value = null
  stopLnClaimPoll()
  lnAddress.value = ''
  lnAddressLoading.value = false
  lnAddressError.value = false
  lnPendingClaims.value = 0
  lastLnReceiptId.value = 0
  lnWatchStartedAt.value = 0
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
const receiveSuccess = ref<{
  amount: number
  verb: string
  methodLabel: string
  rows: SuccessRow[]
  note?: string
} | null>(null)
const onchainQrCanvas = ref<HTMLCanvasElement | null>(null)
const lightningQrCanvas = ref<HTMLCanvasElement | null>(null)
const arkQrCanvas = ref<HTMLCanvasElement | null>(null)
const lnAddressQrCanvas = ref<HTMLCanvasElement | null>(null)
const processing = ref(false)
const error = ref('')

// ── Minibits Lightning address (ecash receive) ──────────────────────────────
// The ecash tab doubles as "receive onto my @minibits.cash address": the node
// derives/registers it from its own ecash seed (wallet.ecash-lnaddress) and
// sweeps any Lightning payments that land there back into ecash while the tab is
// open (wallet.ecash-lnaddress-claim). A registration failure is never fatal —
// the paste-token path below always works.
const lnAddress = ref('')
const lnAddressLoading = ref(false)
const lnAddressError = ref(false)
const lnAddressNeedsSetup = ref(false)
// A payment the backend fetched (and so already consumed at Minibits) but
// couldn't redeem yet — it's queued for automatic retry, not lost, but the
// operator should see it rather than have it be a silent, unbounded wait.
const lnPendingClaims = ref(0)
const lastLnReceiptId = ref(0)
// A durable receipt is shared across browser contexts. Only surface one whose
// redemption happened after this receive watch began; reopening the modal must
// not replay the previous payment as though it were new.
const lnWatchStartedAt = ref(0)
let lnClaimTimer: ReturnType<typeof setInterval> | null = null
let lnClaimInFlight = false

async function loadLnAddress() {
  if (lnAddress.value || lnAddressLoading.value) return
  lnAddressLoading.value = true
  lnAddressError.value = false
  lnAddressNeedsSetup.value = false
  try {
    const res = await rpcClient.call<{ address?: string }>({
      method: 'wallet.ecash-lnaddress',
      timeout: 60_000,
    })
    lnAddress.value = res?.address || ''
    if (lnAddress.value) {
      await nextTick()
      renderQr(lnAddress.value, lnAddressQrCanvas.value)
      startLnClaimPoll()
    } else {
      lnAddressError.value = true
    }
  } catch {
    lnAddressError.value = true
    // A legacy wallet may hold valid proofs without having a recovery phrase.
    // Use the existing authenticated setup flow; never silently create a new
    // identity or send the user to an unexplained generic service error.
    try {
      const seedStatus = await rpcClient.call<{ active: boolean; can_activate: boolean }>({
        method: 'wallet.ecash-seed-status',
        timeout: 5000,
      })
      lnAddressNeedsSetup.value = seedStatus.active === false && seedStatus.can_activate === true
    } catch { /* Keep the retryable service error when status is unavailable. */ }
  } finally {
    lnAddressLoading.value = false
  }
}

function stopLnClaimPoll() {
  if (lnClaimTimer) {
    clearInterval(lnClaimTimer)
    lnClaimTimer = null
  }
}

function startLnClaimPoll() {
  stopLnClaimPoll()
  // Do not make a newly opened receive screen wait a full interval before it
  // can discover a payment (or a receipt claimed by another active client).
  void pollLnClaims()
  lnClaimTimer = setInterval(() => void pollLnClaims(), 3000)
}

async function pollLnClaims() {
  if (lnClaimInFlight) return
  if (!props.show || !lnAddress.value) {
    stopLnClaimPoll()
    return
  }
  lnClaimInFlight = true
  try {
    const res = await rpcClient.call<{
      received_sats?: number
      failed_count?: number
      receipt_id?: number
      receipt_sats?: number
      receipt_at?: number
    }>({
      method: 'wallet.ecash-lnaddress-claim',
      // Relay collection alone has a ten-second window and redemption may
      // then contact the mint. Keep the browser request alive long enough for
      // the backend's bounded work instead of timing out and queuing another.
      timeout: 90_000,
    })
    // The user may have closed the modal while the relay fetch was in flight.
    // Do not resurrect its status or emit a stale received event afterward.
    if (!props.show || !lnAddress.value) return
    const receiptId = res?.receipt_id || 0
    const isNewReceipt = receiptId > 0 && receiptId !== lastLnReceiptId.value
    const receiptAt = res?.receipt_at || 0
    const receiptAgeSeconds = Math.floor(Date.now() / 1000) - receiptAt
    const happenedDuringThisWatch = receiptAt >= Math.max(0, lnWatchStartedAt.value - 1)
    const recentSharedReceipt = isNewReceipt
      && happenedDuringThisWatch
      && receiptAgeSeconds >= 0
      && receiptAgeSeconds <= 180
    const receivedSats = res?.received_sats && res.received_sats > 0
      ? res.received_sats
      : recentSharedReceipt ? (res?.receipt_sats || 0) : 0
    if (receiptId > 0) lastLnReceiptId.value = receiptId
    if (receivedSats > 0) {
      receiveSuccess.value = {
        amount: receivedSats,
        verb: 'RECEIVED',
        methodLabel: 'Lightning address to Cashu',
        rows: [{ label: 'Lightning address', value: lnAddress.value }],
        note: 'The payment was claimed into this node\'s Cashu balance.',
      }
      stopLnClaimPoll()
      emit('received')
    }
    lnPendingClaims.value = res?.failed_count || 0
  } catch {
    // Transient poll failure (offline, mint busy) — keep polling.
  } finally {
    lnClaimInFlight = false
  }
}

onUnmounted(stopLnClaimPoll)

// Fetch the address the first time the operator opens the ecash tab.
watch([receiveMethod, () => props.show], ([m, open]) => {
  if (m === 'ecash' && open) {
    lnWatchStartedAt.value = Math.floor(Date.now() / 1000)
    void loadLnAddress()
  }
})

// ── On-chain payment detection ────────────────────────────────────────────
// The generated address is FRESH (lnd.newaddress), so any incoming wallet
// transaction paying it is this receive — no baseline bookkeeping needed.
// Poll while the QR is showing; flip to the success view on first sight
// (0-conf, clock), keep polling gently until the first confirmation
// upgrades it to a check, then stop.
const paymentSeen = ref<null | { txid: string; amountSats: number; confirmations: number }>(null)
let watchTimer: ReturnType<typeof setInterval> | null = null

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
    receiveSuccess.value = {
      amount: hit.amount,
      verb: 'RECEIVED',
      methodLabel: hit.num_confirmations > 0 ? 'Confirmed on-chain' : 'Received on-chain',
      rows: [{ label: t('receiveBitcoin.transactionId'), value: hit.tx_hash }],
      note: hit.num_confirmations > 0
        ? 'The transaction has its first confirmation.'
        : t('receiveBitcoin.paymentBroadcastHint'),
    }
    if (firstSighting) emit('received')
    if (hit.num_confirmations > 0) stopWatchingPayment()
  } catch {
    // Transient poll failure (daemon busy, LND mid-restart) — keep watching.
  }
}

onUnmounted(stopWatchingPayment)

// Lightning settlement watch — the on-chain pattern minus the broadcast
// step: an invoice is either open or SETTLED (final), so the success view
// goes straight to the green check.
const invoicePaid = ref<null | { amountSats: number }>(null)
const invoiceRHash = ref('')
let invoiceTimer: ReturnType<typeof setInterval> | null = null

function stopWatchingInvoice() {
  if (invoiceTimer) {
    clearInterval(invoiceTimer)
    invoiceTimer = null
  }
}

function startWatchingInvoice() {
  stopWatchingInvoice()
  invoiceTimer = setInterval(() => void checkInvoice(), 3000)
}

async function checkInvoice() {
  if (!props.show || !invoiceRHash.value) {
    stopWatchingInvoice()
    return
  }
  try {
    const res = await rpcClient.call<{ settled: boolean; amt_paid_sat: number }>({
      method: 'lnd.invoicestatus',
      params: { r_hash_hex: invoiceRHash.value },
    })
    if (!res.settled) return
    invoicePaid.value = { amountSats: res.amt_paid_sat || invoiceAmount.value }
    receiveSuccess.value = {
      amount: res.amt_paid_sat || invoiceAmount.value,
      verb: 'RECEIVED',
      methodLabel: 'Received over Lightning',
      rows: invoiceRHash.value ? [{ label: 'Payment hash', value: invoiceRHash.value }] : [],
    }
    stopWatchingInvoice()
    emit('received')
  } catch {
    // Transient poll failure — keep watching.
  }
}

onUnmounted(stopWatchingInvoice)

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
  stopWatchingInvoice()
  stopLnClaimPoll()
  paymentSeen.value = null
  invoiceResult.value = ''
  onchainAddress.value = ''
  arkAddress.value = ''
  ecashToken.value = ''
  receiveSuccess.value = null
  lnAddress.value = ''
  lnPendingClaims.value = 0
  lastLnReceiptId.value = 0
  lnWatchStartedAt.value = 0
  error.value = ''
  emit('close')
}

/** Return to a clean form after a receipt without carrying a spent token or address forward. */
function receiveAnother() {
  receiveSuccess.value = null
  error.value = ''
  processing.value = false
  if (receiveMethod.value === 'lightning') {
    stopWatchingInvoice()
    invoicePaid.value = null
    invoiceRHash.value = ''
    invoiceResult.value = ''
    invoiceAmount.value = 0
    invoiceMemo.value = ''
  } else if (receiveMethod.value === 'onchain') {
    stopWatchingPayment()
    paymentSeen.value = null
    onchainAddress.value = ''
  } else if (receiveMethod.value === 'ecash') {
    ecashToken.value = ''
    if (lnAddress.value) startLnClaimPoll()
  } else {
    arkAddress.value = ''
  }
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
      const res = await rpcClient.call<{ payment_request: string; r_hash_hex?: string }>({
        method: 'lnd.createinvoice',
        params: { amount_sats: invoiceAmount.value, memo: invoiceMemo.value || undefined },
      })
      invoiceResult.value = res.payment_request
      invoiceRHash.value = res.r_hash_hex || ''
      invoicePaid.value = null
      if (invoiceRHash.value) startWatchingInvoice()
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
      const token = ecashToken.value.trim()
      const res = await rpcClient.call<{ received_sats?: number; kind?: string; mint_url?: string }>({
        method: 'wallet.ecash-receive',
        params: { token },
      })
      const kind = res.kind === 'fedimint' ? 'Fedimint' : 'Cashu'
      const rows: SuccessRow[] = []
      if (res.mint_url) rows.push({ label: 'Mint', value: res.mint_url })
      rows.push({
        label: kind === 'Fedimint' ? 'Notes redeemed' : 'Token redeemed',
        value: token,
        hint: 'Keep this if you ever need to show what was redeemed.',
        truncate: true,
      })
      receiveSuccess.value = {
        amount: res.received_sats || 0,
        verb: 'RECEIVED',
        methodLabel: kind,
        rows,
      }
      ecashToken.value = ''
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
