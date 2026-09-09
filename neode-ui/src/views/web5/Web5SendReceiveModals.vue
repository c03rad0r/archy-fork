<template>
  <!-- Unified Send Modal -->
  <Teleport to="body">
  <div v-if="showUnifiedSendModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="closeUnifiedSendModal" @keydown.escape="closeUnifiedSendModal">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="send-bitcoin-title">
      <h2 id="send-bitcoin-title" class="text-lg font-bold text-white mb-4">{{ t('web5.sendBitcoinTitle') }}</h2>

      <PaymentSuccessPane
        v-if="sendSuccess"
        :amount="sendSuccess.amount"
        verb="SENT"
        :method-label="sendSuccess.methodLabel"
        :rows="sendSuccess.rows"
        :note="sendSuccess.note"
        again-label="Send another"
        @again="sendAnother"
        @done="closeUnifiedSendModal"
      />

      <template v-else>
      <!-- Method tabs -->
      <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
        <button
          v-for="m in (['auto', 'lightning', 'onchain', 'ecash'] as const)"
          :key="m"
          @click="sendMethod = m"
          class="flex-1 px-2 py-1.5 rounded text-xs font-medium capitalize transition-colors"
          :class="sendMethod === m ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
        >{{ m === 'onchain' ? 'On-chain' : m }}</button>
      </div>

      <div v-if="sendMethod === 'auto'" class="mb-3 p-2 bg-white/5 rounded-lg">
        <p class="text-xs text-white/50">Auto-selects method based on amount: ecash &lt; 1k sats, Lightning 1k-500k, on-chain &gt; 500k</p>
      </div>

      <div class="mb-3">
        <label class="text-white/60 text-sm block mb-1">Amount (sats)</label>
        <input v-model.number="unifiedSendAmount" type="number" min="1" placeholder="1000" class="w-full input-glass" />
      </div>

      <div v-if="effectiveSendMethod !== 'ecash'" class="mb-3">
        <label class="text-white/60 text-sm block mb-1">
          {{ effectiveSendMethod === 'lightning' ? 'Lightning Invoice (BOLT11)' : 'Bitcoin Address' }}
        </label>
        <textarea v-model="unifiedSendDest" rows="2" :placeholder="effectiveSendMethod === 'lightning' ? 'lnbc...' : 'bc1...'" class="w-full input-glass font-mono"></textarea>
      </div>

      <div v-if="effectiveSendMethod === 'onchain'" class="mb-3 flex items-center gap-3 p-3 bg-white/5 rounded-lg">
        <label class="relative inline-flex items-center cursor-pointer">
          <input type="checkbox" v-model="useHardwareWallet" class="sr-only peer" />
          <div class="w-9 h-5 bg-white/10 peer-focus:outline-none rounded-full peer peer-checked:bg-orange-500/40 transition-colors after:content-[''] after:absolute after:top-0.5 after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:after:translate-x-full"></div>
        </label>
        <div>
          <p class="text-sm text-white">{{ t('web5.signWithHwWallet') }}</p>
          <p class="text-xs text-white/40">{{ t('web5.createsPsbt') }}</p>
        </div>
      </div>

      <div v-if="psbtStep === 'created' && psbtData" class="mb-3 space-y-2">
        <div class="p-3 bg-white/5 rounded-lg">
          <p class="text-xs text-white/50 mb-1">Unsigned PSBT (copy or download):</p>
          <textarea readonly :value="psbtData" rows="3" class="w-full bg-black/20 border border-white/10 rounded px-2 py-1 text-xs font-mono text-white/80 focus:outline-none"></textarea>
          <div class="flex gap-2 mt-2">
            <button @click="copyPsbt" class="text-xs text-orange-400 hover:text-orange-300">Copy PSBT</button>
            <button @click="downloadPsbt" class="text-xs text-orange-400 hover:text-orange-300">Download .psbt</button>
          </div>
        </div>
        <div class="p-3 bg-white/5 rounded-lg">
          <p class="text-xs text-white/50 mb-1">Paste signed PSBT or upload file:</p>
          <textarea v-model="signedPsbtInput" rows="3" placeholder="Paste signed PSBT base64 here..." class="w-full bg-black/20 border border-white/10 rounded px-2 py-1 text-xs font-mono text-white/80 focus:outline-none focus:border-white/30"></textarea>
          <div class="flex gap-2 mt-2">
            <label class="text-xs text-orange-400 hover:text-orange-300 cursor-pointer">
              Upload .psbt
              <input type="file" accept=".psbt,.txt" class="hidden" @change="handlePsbtFileUpload" />
            </label>
          </div>
        </div>
      </div>

      <div v-if="showMeshRelayPrompt" class="mb-3 alert-warning">
        <div class="flex items-center gap-2 mb-2">
          <span class="text-lg">&#x1F4E1;</span>
          <p class="text-orange-300 text-sm font-medium">You are offline</p>
        </div>
        <p class="text-white/70 text-xs mb-3">Send this transaction via mesh radio? It will be relayed by the nearest internet-connected node and you'll receive confirmation updates.</p>
        <div class="flex gap-2">
          <button @click="dismissMeshRelayPrompt" class="flex-1 glass-button px-3 py-2 rounded-lg text-xs">Cancel</button>
          <button @click="handleMeshRelaySend" class="flex-1 glass-button glass-button-warning px-3 py-2 rounded-lg text-xs font-medium">Send via Mesh</button>
        </div>
      </div>

      <div v-if="meshRelayActive" class="mb-3 alert-warning">
        <div class="flex items-center gap-2 mb-1">
          <svg class="animate-spin h-3 w-3 text-orange-400" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
          <p class="text-orange-300 text-xs font-medium">Mesh Relay</p>
        </div>
        <p class="text-white/60 text-xs">{{ meshRelayStatus }}</p>
      </div>

      <div v-if="unifiedSendError" class="mb-3 text-xs text-red-400">{{ unifiedSendError }}</div>

      <div class="flex gap-3">
        <button @click="closeUnifiedSendModal" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.close') }}</button>
        <button v-if="psbtStep === 'created'" @click="finalizePsbt" :disabled="unifiedSendProcessing || !signedPsbtInput.trim()" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
          {{ unifiedSendProcessing ? 'Broadcasting...' : 'Broadcast' }}
        </button>
        <button v-else @click="unifiedSend" :disabled="unifiedSendProcessing || !unifiedSendAmount" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
          {{ unifiedSendProcessing ? 'Sending...' : (useHardwareWallet && effectiveSendMethod === 'onchain' ? 'Create PSBT' : 'Send') }}
        </button>
      </div>
      </template>
    </div>
  </div>
  </Teleport>

  <!-- Unified Receive Modal -->
  <Teleport to="body">
  <div v-if="showUnifiedReceiveModal" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-md" @click.self="closeUnifiedReceiveModal" @keydown.escape="closeUnifiedReceiveModal">
    <div class="glass-card p-6 w-full max-w-2xl mx-4 max-h-[90vh] overflow-y-auto" role="dialog" aria-modal="true" aria-labelledby="receive-bitcoin-title">
      <h2 id="receive-bitcoin-title" class="text-lg font-bold text-white mb-4">{{ t('web5.receiveBitcoinTitle') }}</h2>

      <!-- The payment's moment — the same screen the on-chain and Lightning
           flows use, so ecash isn't a second-class receipt. Replaces the form
           until dismissed, and carries the identifiers a person would need if
           the payment is ever questioned. -->
      <PaymentSuccessPane
        v-if="ecashSuccess"
        :amount="ecashSuccess.amount"
        :verb="ecashSuccess.verb"
        :method-label="ecashSuccess.methodLabel"
        :rows="ecashSuccess.rows"
        again-label="Receive another"
        @again="ecashSuccess = null"
        @done="closeUnifiedReceiveModal"
      />

      <template v-else>

      <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
        <button
          v-for="m in (['onchain', 'lightning', 'ecash'] as const)"
          :key="m"
          @click="receiveMethod = m"
          class="flex-1 px-2 py-1.5 rounded text-xs font-medium capitalize transition-colors"
          :class="receiveMethod === m ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
        >{{ m === 'onchain' ? 'On-chain' : m }}</button>
      </div>

      <div v-if="receiveMethod === 'lightning'">
        <div class="mb-3">
          <label class="text-white/60 text-sm block mb-1">Amount (sats)</label>
          <input v-model.number="receiveInvoiceAmount" type="number" min="1" placeholder="1000" class="w-full input-glass" />
        </div>
        <div class="mb-3">
          <label class="text-white/60 text-sm block mb-1">Memo (optional)</label>
          <input v-model="receiveInvoiceMemo" type="text" placeholder="Payment for..." class="w-full input-glass" />
        </div>
        <div v-if="receiveInvoiceResult" class="mb-3 p-2 bg-white/5 rounded-lg">
          <p class="text-white/50 text-xs mb-1">Invoice (share with sender):</p>
          <p class="text-xs font-mono text-white/80 break-all">{{ receiveInvoiceResult }}</p>
          <button @click="copyToClipboard(receiveInvoiceResult, 'Invoice copied')" class="mt-2 text-xs text-orange-400 hover:text-orange-300">Copy</button>
        </div>
      </div>

      <div v-if="receiveMethod === 'onchain'">
        <div v-if="receiveOnchainAddress" class="mb-3 p-3 bg-white/5 rounded-lg text-center">
          <canvas ref="onchainQrCanvas" class="mx-auto mb-3 rounded-lg" style="image-rendering: pixelated;"></canvas>
          <p class="text-white/50 text-xs mb-2">Your Bitcoin address:</p>
          <p class="text-sm font-mono text-white/90 break-all">{{ receiveOnchainAddress }}</p>
          <button @click="copyToClipboard(receiveOnchainAddress, 'Address copied')" class="mt-2 text-xs text-orange-400 hover:text-orange-300">Copy</button>
        </div>
        <div v-else class="mb-3 text-center">
          <p class="text-white/50 text-sm mb-2">{{ t('web5.generateFreshAddress') }}</p>
          <p v-if="unifiedReceiveProcessing" class="text-xs text-white/40">Checking Lightning wallet readiness...</p>
        </div>
      </div>

      <div v-if="receiveMethod === 'ecash'">
        <div class="mb-3">
          <label class="text-white/60 text-sm block mb-1">Paste ecash token (Cashu or Fedimint)</label>
          <textarea v-model="ecashReceiveToken" rows="3" placeholder="cashuB… or Fedimint notes" class="w-full input-glass"></textarea>
        </div>
      </div>

      <div v-if="unifiedReceiveError" class="mb-3 text-xs text-red-400">{{ unifiedReceiveError }}</div>

      <div class="flex gap-3">
        <button @click="closeUnifiedReceiveModal" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">{{ t('common.close') }}</button>
        <button @click="unifiedReceive" :disabled="unifiedReceiveProcessing" class="flex-1 glass-button glass-button-success px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
          {{ unifiedReceiveProcessing ? 'Processing...' : receiveMethod === 'onchain' ? 'Generate Address' : receiveMethod === 'lightning' ? 'Create Invoice' : 'Receive' }}
        </button>
      </div>
      </template>
    </div>
  </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import PaymentSuccessPane, { type SuccessRow } from '@/components/PaymentSuccessPane.vue'
import { useLightningRequired } from '@/composables/useLightningRequired'
import { useTransportStore } from '@/stores/transport'
import { useMeshStore } from '@/stores/mesh'
import { explainReceiveAddressFailure } from '@/utils/bitcoinReceive'
import { safeClipboardWrite } from './utils'

const lightning = useLightningRequired()

const { t } = useI18n()
const transportStore = useTransportStore()
const meshStore = useMeshStore()

const emit = defineEmits<{
  toast: [text: string]
  balancesChanged: []
}>()

// Send state
const showUnifiedSendModal = ref(false)
const sendMethod = ref<'auto' | 'lightning' | 'onchain' | 'ecash'>('auto')
const unifiedSendAmount = ref<number>(0)
const unifiedSendDest = ref('')
const unifiedSendProcessing = ref(false)
const unifiedSendError = ref('')
const sendResultTxid = ref('')
const sendResultHash = ref('')
const useHardwareWallet = ref(false)
const meshRelayActive = ref(false)
const meshRelayStatus = ref('')
const meshRelayRequestId = ref(0)
const showMeshRelayPrompt = ref(false)
const psbtData = ref('')
const psbtStep = ref<'idle' | 'created' | 'finalizing'>('idle')
const signedPsbtInput = ref('')
const sendSuccess = ref<{
  amount: number
  methodLabel: string
  rows: SuccessRow[]
  note?: string
} | null>(null)

// Receive state
const showUnifiedReceiveModal = ref(false)
const receiveMethod = ref<'lightning' | 'onchain' | 'ecash'>('onchain')
const receiveInvoiceAmount = ref<number>(0)
const receiveInvoiceMemo = ref('')
const receiveInvoiceResult = ref('')
const receiveOnchainAddress = ref('')
const onchainQrCanvas = ref<HTMLCanvasElement | null>(null)
const unifiedReceiveProcessing = ref(false)
const unifiedReceiveError = ref('')
const ecashReceiveToken = ref('')
// Details of the last successful ecash receive, for the success screen.
// Null = nothing to celebrate yet, so the form shows.
const ecashSuccess = ref<{
  amount: number
  verb: string
  methodLabel: string
  rows: SuccessRow[]
} | null>(null)

const effectiveSendMethod = computed(() => {
  if (sendMethod.value !== 'auto') return sendMethod.value
  const amt = unifiedSendAmount.value || 0
  if (amt <= 0) return 'lightning'
  if (amt < 1000) return 'ecash'
  if (amt > 500000) return 'onchain'
  return 'lightning'
})

function openSend() {
  sendSuccess.value = null
  showUnifiedSendModal.value = true
}
function openReceive() { showUnifiedReceiveModal.value = true }

function closeUnifiedSendModal() {
  showUnifiedSendModal.value = false
  sendSuccess.value = null
  unifiedSendError.value = ''
  sendResultTxid.value = ''
  sendResultHash.value = ''
  psbtData.value = ''
  psbtStep.value = 'idle'
  signedPsbtInput.value = ''
}

function closeUnifiedReceiveModal() {
  showUnifiedReceiveModal.value = false
  receiveInvoiceResult.value = ''
  receiveOnchainAddress.value = ''
  ecashReceiveToken.value = ''
  ecashSuccess.value = null
  unifiedReceiveError.value = ''
}

function copyToClipboard(text: string, msg: string) {
  safeClipboardWrite(text)
  emit('toast', msg)
}

async function unifiedSend() {
  if (!unifiedSendAmount.value || unifiedSendProcessing.value) return
  unifiedSendProcessing.value = true
  unifiedSendError.value = ''
  sendSuccess.value = null
  sendResultTxid.value = ''
  sendResultHash.value = ''
  meshRelayActive.value = false
  meshRelayStatus.value = ''

  const method = effectiveSendMethod.value
  try {
    if (method === 'ecash') {
      const res = await rpcClient.call<{ token: string }>({
        method: 'wallet.ecash-send',
        params: { amount_sats: unifiedSendAmount.value },
      })
      sendSuccess.value = {
        amount: unifiedSendAmount.value,
        methodLabel: 'Sent as Cashu',
        rows: [{
          label: 'Token to share',
          value: res.token,
          hint: 'The recipient needs this token to claim the sats. Keep it until they confirm receipt.',
          truncate: true,
        }],
      }
    } else if (method === 'lightning') {
      if (!unifiedSendDest.value.trim()) {
        unifiedSendError.value = t('web5.pasteInvoice')
        return
      }
      // Waits out slow multi-hop routing and only reports failure when LND
      // itself declares the payment failed — never on a timeout.
      const res = await rpcClient.payLightningInvoice({ payment_request: unifiedSendDest.value.trim() })
      if (res.status === 'failed') throw new Error(res.failure_reason || 'Payment failed')
      sendResultHash.value = res.payment_hash
      sendSuccess.value = {
        amount: res.amount_sats || unifiedSendAmount.value,
        methodLabel: res.status === 'pending' ? 'Payment in flight' : 'Paid over Lightning',
        rows: res.payment_hash ? [{ label: 'Payment hash', value: res.payment_hash }] : [],
        ...(res.status === 'pending'
          ? { note: 'This payment is taking longer than usual to settle. Check transactions before retrying.' }
          : {}),
      }
    } else {
      if (!unifiedSendDest.value.trim()) {
        unifiedSendError.value = t('web5.enterBitcoinAddress')
        return
      }
      if (useHardwareWallet.value) {
        const res = await rpcClient.createPsbt({
          outputs: [{ address: unifiedSendDest.value.trim(), amount_sats: unifiedSendAmount.value }],
        })
        psbtData.value = res.psbt_base64
        psbtStep.value = 'created'
        signedPsbtInput.value = ''
        unifiedSendProcessing.value = false
        return
      }
      await transportStore.fetchStatus()
      if (transportStore.meshOnly) {
        showMeshRelayPrompt.value = true
        unifiedSendProcessing.value = false
        return
      }
      try {
        const res = await rpcClient.call<{ txid: string }>({
          method: 'lnd.sendcoins',
          params: { addr: unifiedSendDest.value.trim(), amount: unifiedSendAmount.value },
        })
        sendResultTxid.value = res.txid
        sendSuccess.value = {
          amount: unifiedSendAmount.value,
          methodLabel: 'Sent on-chain',
          rows: [{ label: 'Transaction ID', value: res.txid }],
          note: 'The transaction has been broadcast and will confirm over the next blocks.',
        }
      } catch (sendErr: unknown) {
        const errMsg = sendErr instanceof Error ? sendErr.message : ''
        if (errMsg.includes('connection') || errMsg.includes('timeout') || errMsg.includes('unavailable')) {
          showMeshRelayPrompt.value = true
          unifiedSendProcessing.value = false
          return
        }
        throw sendErr
      }
    }
    emit('balancesChanged')
  } catch (err: unknown) {
    unifiedSendError.value = err instanceof Error ? err.message : t('web5.sendFailed')
  } finally {
    unifiedSendProcessing.value = false
  }
}

async function handleMeshRelaySend() {
  showMeshRelayPrompt.value = false
  unifiedSendProcessing.value = true
  meshRelayActive.value = true
  meshRelayStatus.value = 'Creating signed transaction...'
  unifiedSendError.value = ''
  try {
    meshRelayStatus.value = 'Signing transaction locally...'
    const rawRes = await rpcClient.call<{ raw_tx_hex: string; amount_sats: number }>({
      method: 'lnd.create-raw-tx',
      params: { addr: unifiedSendDest.value.trim(), amount_sats: unifiedSendAmount.value },
    })
    meshRelayStatus.value = 'Sending via mesh radio to connected peers...'
    const relayRes = await meshStore.relayTransaction(rawRes.raw_tx_hex)
    meshRelayRequestId.value = relayRes.request_id
    meshRelayStatus.value = 'Transaction sent via mesh -- waiting for broadcast confirmation...'
    startMeshRelayPolling(relayRes.request_id)
  } catch (err: unknown) {
    meshRelayActive.value = false
    meshRelayStatus.value = ''
    unifiedSendError.value = err instanceof Error ? err.message : 'Mesh relay failed'
  } finally {
    unifiedSendProcessing.value = false
  }
}

function dismissMeshRelayPrompt() {
  showMeshRelayPrompt.value = false
}

let meshRelayPollTimer: ReturnType<typeof setInterval> | null = null

function startMeshRelayPolling(_requestId: number) {
  if (meshRelayPollTimer) clearInterval(meshRelayPollTimer)
  meshRelayPollTimer = setInterval(async () => {
    await meshStore.fetchMessages()
    const msgs = meshStore.messages
    for (const msg of msgs) {
      if (msg.direction !== 'received') continue
      const text = msg.plaintext
      if (text.includes(`[tx_relay_response]`) && text.includes('txid:')) {
        const match = text.match(/txid:\s*(\w+)/)
        if (match && match[1]) {
          sendResultTxid.value = match[1]
          sendSuccess.value = {
            amount: unifiedSendAmount.value,
            methodLabel: 'Sent on-chain over mesh',
            rows: [{ label: 'Transaction ID', value: match[1] }],
            note: 'Broadcast confirmed. The transaction is waiting for block confirmations.',
          }
          meshRelayStatus.value = `Broadcast confirmed! txid: ${match[1].slice(0, 16)}... -- waiting for confirmations`
        }
      }
      if (text.includes('[tx_confirmation]')) {
        const confMatch = text.match(/(\d)\/3 confirmations/)
        if (confMatch && confMatch[1]) {
          const confs = parseInt(confMatch[1])
          meshRelayStatus.value = `${confs}/3 confirmations${confs >= 3 ? ' -- Transaction confirmed!' : '...'}`
          if (confs >= 3) {
            meshRelayActive.value = false
            if (meshRelayPollTimer) {
              clearInterval(meshRelayPollTimer)
              meshRelayPollTimer = null
            }
            emit('balancesChanged')
          }
        }
      }
    }
  }, 5000)
  setTimeout(() => {
    if (meshRelayPollTimer) {
      clearInterval(meshRelayPollTimer)
      meshRelayPollTimer = null
    }
  }, 3 * 60 * 60 * 1000)
}

async function finalizePsbt() {
  if (!signedPsbtInput.value.trim() || unifiedSendProcessing.value) return
  unifiedSendProcessing.value = true
  unifiedSendError.value = ''
  try {
    await rpcClient.finalizePsbt(signedPsbtInput.value.trim())
    psbtStep.value = 'idle'
    psbtData.value = ''
    signedPsbtInput.value = ''
    sendResultTxid.value = t('web5.broadcastViaHwWallet')
    sendSuccess.value = {
      amount: unifiedSendAmount.value,
      methodLabel: 'Sent on-chain with hardware wallet',
      rows: [],
      note: t('web5.broadcastViaHwWallet'),
    }
    emit('balancesChanged')
  } catch (err: unknown) {
    unifiedSendError.value = err instanceof Error ? err.message : t('web5.broadcastFailed')
  } finally {
    unifiedSendProcessing.value = false
  }
}

function sendAnother() {
  sendSuccess.value = null
  unifiedSendAmount.value = 0
  unifiedSendDest.value = ''
  unifiedSendError.value = ''
  sendResultTxid.value = ''
  sendResultHash.value = ''
  psbtData.value = ''
  psbtStep.value = 'idle'
  signedPsbtInput.value = ''
  useHardwareWallet.value = false
}

function copyPsbt() {
  if (!psbtData.value) return
  safeClipboardWrite(psbtData.value)
  unifiedSendError.value = t('web5.psbtCopied')
}

function downloadPsbt() {
  if (!psbtData.value) return
  const blob = new Blob([psbtData.value], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'transaction.psbt'
  a.click()
  URL.revokeObjectURL(url)
}

function handlePsbtFileUpload(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  const reader = new FileReader()
  reader.onload = (e) => {
    signedPsbtInput.value = (e.target?.result as string) || ''
  }
  reader.readAsText(file)
  input.value = ''
}

async function unifiedReceive() {
  if (unifiedReceiveProcessing.value) return
  unifiedReceiveProcessing.value = true
  unifiedReceiveError.value = ''
  try {
    if (receiveMethod.value === 'lightning') {
      // Missing prerequisite, not an error — raise the install modal rather
      // than letting lnd.createinvoice fail with connection-refused.
      if (!(await lightning.requireLightningReady('receive'))) return
      if (!receiveInvoiceAmount.value || receiveInvoiceAmount.value < 1) {
        unifiedReceiveError.value = t('web5.enterAmount')
        return
      }
      const res = await rpcClient.call<{ payment_request: string }>({
        method: 'lnd.createinvoice',
        params: { amount_sats: receiveInvoiceAmount.value, memo: receiveInvoiceMemo.value },
      })
      receiveInvoiceResult.value = res.payment_request
    } else if (receiveMethod.value === 'onchain') {
      const res = await rpcClient.call<{ address: string }>({ method: 'lnd.newaddress' })
      if (!res.address) {
        throw new Error('LND did not return a Bitcoin address')
      }
      receiveOnchainAddress.value = res.address
      nextTick(() => renderQrCode(res.address, onchainQrCanvas.value))
    } else {
      if (!ecashReceiveToken.value.trim()) {
        unifiedReceiveError.value = t('web5.pasteEcashToken')
        return
      }
      const res = await rpcClient.call<{ received_sats: number; kind?: string; mint_url?: string }>({
        method: 'wallet.ecash-receive',
        params: { token: ecashReceiveToken.value.trim() },
      })
      const label = res.kind === 'fedimint' ? 'Fedimint' : 'Cashu'
      // Ecash leaves no public ledger entry behind, so the issuer and the
      // redeemed token are the only things a person can quote later if the
      // payment is ever questioned. Capture them before clearing the box.
      const rows: SuccessRow[] = []
      if (res.mint_url) rows.push({ label: 'Mint', value: res.mint_url })
      rows.push({
        label: label === 'Fedimint' ? 'Notes redeemed' : 'Token redeemed',
        value: ecashReceiveToken.value.trim(),
        hint: 'Keep this if you ever need to show what you redeemed.',
        truncate: true,
      })
      ecashSuccess.value = { amount: res.received_sats, verb: 'RECEIVED', methodLabel: label, rows }
      ecashReceiveToken.value = ''
      emit('balancesChanged')
    }
  } catch (err: unknown) {
    unifiedReceiveError.value = receiveMethod.value === 'onchain'
      ? explainReceiveAddressFailure(err)
      : err instanceof Error ? err.message : t('web5.receiveFailed')
  } finally {
    unifiedReceiveProcessing.value = false
  }
}

async function renderQrCode(data: string, canvas: HTMLCanvasElement | null) {
  if (!canvas || !data) return
  try {
    const QRCode = await import('qrcode')
    await QRCode.toCanvas(canvas, `bitcoin:${data}`, {
      width: 200,
      margin: 2,
      color: { dark: '#000000', light: '#ffffff' },
    })
  } catch { /* QR rendering failed silently */ }
}

defineExpose({
  openSend,
  openReceive,
  meshRelayActive,
  meshRelayStatus,
  sendResultTxid,
})
</script>
