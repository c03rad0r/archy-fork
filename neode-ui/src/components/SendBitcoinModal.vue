<template>
  <BaseModal :show="show" :title="t('web5.sendBitcoinTitle')" max-width="max-w-2xl" content-class="max-h-[90vh] overflow-y-auto" @close="close">
        <!-- ============ SUCCESS PANE — the payment's moment, not a footnote ============ -->
        <template v-if="successInfo">
          <div class="text-center py-4">
            <div class="send-success-badge mx-auto mb-6">
              <ScreensaverRing size="badge" />
              <div class="send-success-burst">
                <div class="burst-core">
                  <svg class="w-14 h-14 text-green-400 burst-check" fill="none" stroke="currentColor" stroke-width="3" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
              </div>
            </div>

            <div v-if="successInfo.amount > 0" class="text-5xl font-black text-green-400 mb-1">
              {{ successInfo.amount.toLocaleString() }}<span class="text-2xl font-bold text-green-400/70"> sats</span>
            </div>
            <div class="text-2xl font-bold tracking-widest text-white mb-1">SENT</div>
            <p class="text-sm text-white/50 mb-6">{{ successInfo.methodLabel }}</p>

            <div v-if="successInfo.hash || successInfo.txid || successInfo.note" class="p-4 bg-white/5 rounded-xl text-left space-y-4 mb-6">
              <div v-if="successInfo.hash">
                <p class="text-xs text-white/50 mb-1">Payment hash</p>
                <div class="flex items-center gap-2">
                  <p class="flex-1 text-xs font-mono text-white/80 break-all">{{ successInfo.hash }}</p>
                  <CopyButton class="shrink-0" :value="successInfo.hash" />
                </div>
              </div>
              <div v-if="successInfo.txid">
                <p class="text-xs text-white/50 mb-1">Transaction ID</p>
                <div class="flex items-center gap-2">
                  <p class="flex-1 text-xs font-mono text-white/80 break-all">{{ successInfo.txid }}</p>
                  <CopyButton class="shrink-0" :value="successInfo.txid" />
                </div>
              </div>
              <p v-if="successInfo.note" class="text-xs text-white/60">{{ successInfo.note }}</p>
            </div>

            <div class="flex gap-3">
              <button @click="sendAnother" class="flex-1 glass-button px-4 py-3 rounded-xl text-sm font-medium">Send another</button>
              <button @click="close" class="flex-1 glass-button glass-button-warning px-4 py-3 rounded-xl text-sm font-semibold">Done</button>
            </div>
          </div>
        </template>

        <!-- ============ CONFIRM PANE (second step, mirrors the scan flow) ============ -->
        <template v-else-if="confirming">
          <div class="mb-3 p-3 bg-white/5 rounded-lg">
            <div class="flex items-center justify-between mb-2">
              <span class="text-xs text-white/50">Method</span>
              <span class="text-sm font-medium" :class="methodColor">{{ methodLabel }}</span>
            </div>
            <div v-if="dest.trim()" class="mb-1">
              <p class="text-xs text-white/50 mb-1">{{ effectiveMethod === 'lightning' ? 'Invoice' : effectiveMethod === 'ark' ? 'Destination' : 'Address' }}</p>
              <p class="text-xs font-mono text-white/80 break-all">{{ destDisplay }}</p>
            </div>
            <p v-else class="text-xs text-white/60">Creates a {{ methodLabel }} token you can share — sats leave your balance when it's redeemed.</p>
          </div>

          <!-- Live balance impact -->
          <div class="mb-3 p-3 bg-white/5 rounded-lg space-y-1.5">
            <div class="flex items-center justify-between">
              <span class="text-xs text-white/50">{{ methodLabel }} balance</span>
              <span class="text-sm font-medium text-white/80">{{ confirmBalance === null ? '…' : confirmBalance.toLocaleString() + ' sats' }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span class="text-xs text-white/50 flex items-center gap-2">
                Amount
                <span v-if="invoiceAmountSats !== null" class="text-[11px] px-2 py-0.5 rounded-full bg-white/10 text-white/50">set by invoice</span>
              </span>
              <span class="text-sm font-medium text-white/80">−{{ confirmAmount.toLocaleString() }} sats</span>
            </div>
            <div v-if="effectiveMethod === 'onchain'" class="flex items-center justify-between">
              <span class="text-xs text-white/50">Network fee</span>
              <span class="text-sm font-medium text-white/80">{{ feeEstimateLabel }}</span>
            </div>
            <div class="flex items-center justify-between">
              <span class="text-xs text-white/50">Balance after</span>
              <span class="text-sm font-medium" :class="insufficient ? 'text-red-400' : 'text-white/80'">
                {{ confirmBalance === null ? '…' : balanceAfter.toLocaleString() + ' sats' }}
              </span>
            </div>
          </div>
          <p v-if="insufficient" class="text-xs text-red-400 mb-3">Not enough {{ methodLabel }} balance for this amount.</p>
          <p v-else-if="isSweep" class="text-xs text-white/50 mb-3">Sweeps the entire on-chain balance minus network fees.</p>
          <p v-else-if="effectiveMethod === 'onchain'" class="text-xs text-white/50 mb-3">Network fees are deducted on top of the amount.</p>

          <div v-if="error" class="mb-3 alert-error">{{ error }}</div>

          <div class="flex gap-3">
            <button @click="confirming = false" :disabled="processing" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Back</button>
            <button @click="send" :disabled="processing || insufficient || (confirmAmount <= 0 && !isSweep)" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
              {{ processing ? t('common.sending') : 'Confirm & Send' }}
            </button>
          </div>
        </template>

        <template v-else>
        <!-- Method tabs -->
        <div class="flex gap-1 mb-4 p-1 bg-white/5 rounded-lg">
          <button
            v-for="m in (['lightning', 'onchain', 'ecash', 'fedimint', 'ark'] as const)"
            :key="m"
            @click="sendMethod = m"
            class="flex-1 px-2 py-1.5 rounded text-xs font-medium capitalize transition-colors"
            :class="sendMethod === m ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
          >{{ m === 'onchain' ? t('sendBitcoin.onChain') : m === 'lightning' ? t('sendBitcoin.lightning') : m === 'ecash' ? 'Cashu' : m === 'fedimint' ? 'Fedi' : 'Ark' }}</button>
        </div>

        <div v-if="sendMethod === 'auto'" class="mb-3 p-2 bg-white/5 rounded-lg">
          <p class="text-xs text-white/50">{{ t('sendBitcoin.autoMethodDesc') }}</p>
        </div>

        <div class="mb-3">
          <div class="flex items-center justify-between mb-1">
            <div class="flex items-center gap-2">
              <label class="text-white/60 text-sm">{{ amountLabel }}</label>
              <!-- sats/BTC entry toggle (on-chain only) -->
              <div v-if="sendMethod === 'onchain'" class="flex p-0.5 bg-white/5 rounded-md">
                <button
                  v-for="u in (['sats', 'btc'] as const)"
                  :key="u"
                  @click="setAmountUnit(u)"
                  class="px-2 py-0.5 rounded text-[11px] font-medium transition-colors"
                  :class="amountUnit === u ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
                >{{ u === 'btc' ? 'BTC' : 'sats' }}</button>
              </div>
            </div>
            <span v-if="pastedInvoiceAmount !== null" class="text-[11px] px-2 py-0.5 rounded-full bg-white/10 text-white/50">set by invoice</span>
            <button
              v-if="sendMethod === 'onchain'"
              @click="toggleSendAll"
              class="text-xs px-2 py-0.5 rounded border transition-colors"
              :class="sendAll
                ? 'bg-orange-500/20 border-orange-500/40 text-orange-300'
                : 'bg-white/5 border-white/15 text-white/60 hover:text-white/90'"
            >
              Send all funds
            </button>
          </div>
          <input
            v-model.number="amountEntry"
            type="number"
            :min="amountUnit === 'btc' && sendMethod === 'onchain' ? '0.00000001' : '1'"
            :step="amountUnit === 'btc' && sendMethod === 'onchain' ? '0.00000001' : '1'"
            :placeholder="sendAll ? '' : pastedInvoiceAmount !== null ? '' : amountUnit === 'btc' && sendMethod === 'onchain' ? '0.001' : '1000'"
            :disabled="sendAll || pastedInvoiceAmount !== null"
            class="w-full input-glass disabled:opacity-50"
          />
          <p v-if="sendAll" class="text-xs text-white/50 mt-1">
            Sweeps your entire on-chain balance{{ onchainBalance !== null ? ` (~${onchainBalance.toLocaleString()} sats)` : '' }} minus network fees.
          </p>
          <p v-else-if="pastedInvoiceAmount !== null" class="text-xs text-white/50 mt-1">
            This invoice fixes the amount — nothing to type, just review and send.
          </p>
          <p v-else-if="effectiveMethod === 'lightning' && dest.trim()" class="text-xs text-white/50 mt-1">
            Zero-amount invoice — enter how many sats to pay.
          </p>
          <p v-else-if="unitConversionHint" class="text-xs text-white/40 mt-1">{{ unitConversionHint }}</p>
        </div>

        <div v-if="effectiveMethod !== 'ecash'" class="mb-3">
          <div class="flex items-center justify-between mb-1">
            <label class="text-white/60 text-sm">
              {{ effectiveMethod === 'lightning' ? t('sendBitcoin.lightningInvoice') : effectiveMethod === 'ark' ? 'Ark address, invoice or lightning address' : t('sendBitcoin.bitcoinAddress') }}
            </label>
            <button
              v-if="canReadClipboard"
              @click="pasteFromClipboard"
              class="text-xs px-2 py-0.5 rounded border bg-white/5 border-white/15 text-white/60 hover:text-white/90 transition-colors"
            >
              {{ effectiveMethod === 'lightning' ? 'Paste invoice' : 'Paste' }}
            </button>
          </div>
          <textarea v-model="dest" rows="2" :placeholder="effectiveMethod === 'lightning' ? 'lnbc...' : effectiveMethod === 'ark' ? 'tark1… / lnbc… / user@lnaddress' : 'bc1...'" class="w-full input-glass font-mono"></textarea>
        </div>

        <!-- Network fee (on-chain only) -->
        <div v-if="sendMethod === 'onchain'" class="mb-3">
          <label class="text-white/60 text-sm block mb-1">Network fee</label>
          <div class="flex gap-1 p-1 bg-white/5 rounded-lg">
            <button
              v-for="preset in onchainFeePresets"
              :key="preset.key"
              @click="feePreset = preset.key"
              class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
              :class="feePreset === preset.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
            >{{ preset.label }}</button>
          </div>
          <p v-if="feePreset !== 'custom'" class="text-white/40 text-xs mt-1">
            {{ onchainFeePresets.find(p => p.key === feePreset)?.hint }}
          </p>
          <div v-else class="grid grid-cols-2 gap-3 mt-2">
            <div>
              <label class="text-white/60 text-xs block mb-1">Target blocks</label>
              <input
                v-model.number="customConfTarget"
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
                v-model.number="customSatPerVbyte"
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

        <div v-if="ecashToken" class="mb-3 p-2 bg-white/5 rounded-lg">
          <p class="text-white/50 text-xs mb-1">{{ t('sendBitcoin.tokenShareLabel') }}</p>
          <!-- QR so the recipient can scan the token straight off this screen
               (animated multi-frame not needed: qrcode handles these sizes). -->
          <div class="flex justify-center my-2">
            <canvas ref="tokenQrCanvas" class="rounded-lg bg-white p-2"></canvas>
          </div>
          <p class="text-xs font-mono text-white/80 break-all">{{ ecashToken }}</p>
          <button @click="copyText(ecashToken)" class="mt-2 text-xs text-orange-400 hover:text-orange-300">{{ t('common.copy') }}</button>
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
          <button @click="review" :disabled="processing" class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50">
            {{ t('common.send') }}
          </button>
        </div>
        </template>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import { useLightningRequired } from '@/composables/useLightningRequired'
import BaseModal from '@/components/BaseModal.vue'
import CopyButton from '@/components/CopyButton.vue'
import ScreensaverRing from '@/components/ScreensaverRing.vue'

const { t } = useI18n()
const lightning = useLightningRequired()

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: []; sent: []; scan: [] }>()

// 'auto' remains in the type for the effectiveMethod logic but is no longer
// offered as a tab (hidden per operator request 2026-07-22).
const sendMethod = ref<'auto' | 'lightning' | 'onchain' | 'ecash' | 'fedimint' | 'ark'>('lightning')

// --- Amount entry with a sats/BTC unit toggle (on-chain). `amountEntry` is
// --- what the user types in the chosen unit; `amount` stays the canonical
// --- sats value the rest of the flow reads and writes.
const amountUnit = ref<'sats' | 'btc'>('sats')
const amountEntry = ref<number>(0)
const amount = computed<number>({
  get: () =>
    amountUnit.value === 'btc'
      ? Math.round((amountEntry.value || 0) * 100_000_000)
      : Math.floor(amountEntry.value || 0),
  set: (sats: number) => {
    amountEntry.value = amountUnit.value === 'btc' ? (sats || 0) / 100_000_000 : sats || 0
  },
})

/** Switch entry unit, converting whatever is already typed. */
function setAmountUnit(unit: 'sats' | 'btc') {
  if (unit === amountUnit.value) return
  const sats = amount.value
  amountUnit.value = unit
  amount.value = sats
}

// Only the on-chain tab offers BTC entry — leaving it snaps back to sats so
// the lightning/ecash flows (and their sats-only hints) stay consistent.
watch(sendMethod, (m) => {
  if (m !== 'onchain' && amountUnit.value !== 'sats') setAmountUnit('sats')
})

const amountLabel = computed(() =>
  sendMethod.value === 'onchain' && amountUnit.value === 'btc' ? 'Amount (BTC)' : t('sendBitcoin.amountSats')
)

const unitConversionHint = computed(() => {
  if (sendMethod.value !== 'onchain' || !amountEntry.value) return ''
  return amountUnit.value === 'btc'
    ? `= ${amount.value.toLocaleString()} sats`
    : `= ${(amount.value / 100_000_000).toFixed(8).replace(/0+$/, '').replace(/\.$/, '')} BTC`
})

const dest = ref('')
const processing = ref(false)
const error = ref('')
// Set on a completed send — flips the modal to the success pane.
const successInfo = ref<{
  amount: number
  methodLabel: string
  hash?: string
  txid?: string
  note?: string
} | null>(null)
const ecashToken = ref('')

// "Send all funds" — sweeps the whole on-chain balance (explicit on-chain tab only)
const sendAll = ref(false)
const onchainBalance = ref<number | null>(null)
const isSweep = computed(() => sendMethod.value === 'onchain' && sendAll.value)

function toggleSendAll() {
  sendAll.value = !sendAll.value
  if (!sendAll.value) {
    // Disarming clears the field — a swept-balance figure left behind reads
    // as a typed amount.
    amount.value = 0
    return
  }
  // Arming shows the swept balance IN the (disabled) amount field — a field
  // stuck at 0 while "send all" is lit read as "sending nothing" (operator
  // feedback 2026-08-05). Refresh the figure on every arm.
  const applyBalance = () => {
    if (sendAll.value && onchainBalance.value !== null) amount.value = onchainBalance.value
  }
  applyBalance()
  rpcClient.call<{ balance_sats: number }>({ method: 'lnd.getinfo', timeout: 5000 })
    .then((res) => { onchainBalance.value = res.balance_sats || 0; applyBalance() })
    .catch(() => { /* balance hint is best-effort */ })
}

// Leaving the on-chain tab disarms the sweep so it can never apply elsewhere
// (and drops the swept-balance figure it wrote into the amount field).
watch(sendMethod, (m) => {
  if (m !== 'onchain' && sendAll.value) {
    sendAll.value = false
    amount.value = 0
  }
})

// Every open starts from a blank slate. Stale state from the previous send —
// destination, amount, and above all an armed "send all funds" toggle — is
// dangerous to inherit invisibly (operator feedback 2026-08-05).
watch(() => props.show, (shown) => {
  if (!shown) return
  sendMethod.value = 'lightning'
  amountUnit.value = 'sats'
  amountEntry.value = 0
  dest.value = ''
  error.value = ''
  successInfo.value = null
  ecashToken.value = ''
  sendAll.value = false
  onchainBalance.value = null
  feePreset.value = 'standard'
  customConfTarget.value = null
  customSatPerVbyte.value = null
  resolvedFeeParams.value = {}
  feeEstimate.value = null
  confirming.value = false
  confirmBalance.value = null
  invoiceAmountSats.value = null
  processing.value = false
})

// --- On-chain network fee: presets map to LND confirmation targets; custom
// --- takes a block target or an explicit sat/vB rate (rate wins).
type OnchainFeePreset = 'fast' | 'standard' | 'slow' | 'custom'

const onchainFeePresets: { key: OnchainFeePreset; label: string; hint?: string; confTarget?: number }[] = [
  { key: 'fast', label: 'Fast', hint: 'Targets the next block (~10 minutes)', confTarget: 1 },
  { key: 'standard', label: 'Standard', hint: 'Confirms within ~6 blocks (about an hour)', confTarget: 6 },
  { key: 'slow', label: 'Slow', hint: 'Confirms within ~144 blocks (about a day)', confTarget: 144 },
  { key: 'custom', label: 'Custom' },
]

const feePreset = ref<OnchainFeePreset>('standard')
const customConfTarget = ref<number | null>(null)
const customSatPerVbyte = ref<number | null>(null)
// Resolved at review time so confirm + send use the same params.
const resolvedFeeParams = ref<{ target_conf?: number; sat_per_vbyte?: number }>({})

function onchainFeeParams(): { target_conf?: number; sat_per_vbyte?: number } | null {
  if (feePreset.value !== 'custom') {
    return { target_conf: onchainFeePresets.find(p => p.key === feePreset.value)?.confTarget ?? 6 }
  }
  const rate = customSatPerVbyte.value
  const conf = customConfTarget.value
  if (rate != null && rate !== 0) {
    if (rate < 1 || rate > 5000) { error.value = 'Sats per vByte must be between 1 and 5000'; return null }
    return { sat_per_vbyte: Math.floor(rate) }
  }
  if (conf != null && conf !== 0) {
    if (conf < 1 || conf > 1008) { error.value = 'Target blocks must be between 1 and 1008'; return null }
    return { target_conf: Math.floor(conf) }
  }
  error.value = 'Custom fee requires target blocks or sats per vByte'
  return null
}

// Fee estimate for the confirm pane (best-effort — LND's own estimator).
const feeEstimate = ref<{ fee_sat: number; sat_per_vbyte: number } | null>(null)
const feeEstimateLoading = ref(false)

const feeEstimateLabel = computed(() => {
  if (feeEstimate.value) {
    return `~${feeEstimate.value.fee_sat.toLocaleString()} sats · ${feeEstimate.value.sat_per_vbyte} sat/vB`
  }
  if (resolvedFeeParams.value.sat_per_vbyte) {
    return `${resolvedFeeParams.value.sat_per_vbyte} sat/vB (custom)`
  }
  if (feeEstimateLoading.value) return '…'
  return isSweep.value ? 'deducted from swept amount' : 'estimated at broadcast'
})

async function loadFeeEstimate() {
  feeEstimate.value = null
  // Explicit sat/vB shows as-is; sweeps have no fixed amount to estimate on.
  if (resolvedFeeParams.value.sat_per_vbyte || isSweep.value) return
  const addr = dest.value.trim()
  const amt = confirmAmount.value
  if (!addr || amt < 546) return
  feeEstimateLoading.value = true
  try {
    const res = await rpcClient.call<{ fee_sat: number; sat_per_vbyte: number }>({
      method: 'lnd.estimatefee',
      params: { addr, amount: amt, target_conf: resolvedFeeParams.value.target_conf ?? 6 },
      timeout: 10000,
    })
    if (res.fee_sat > 0) feeEstimate.value = res
  } catch {
    /* estimate is a preview — the label falls back to prose */
  } finally {
    feeEstimateLoading.value = false
  }
}

// Clipboard read needs a secure context (or the companion bridge); hide the
// button where it can't work — the textarea still accepts a manual paste.
const canReadClipboard = typeof navigator !== 'undefined' && !!navigator.clipboard?.readText

async function pasteFromClipboard() {
  try {
    const text = (await navigator.clipboard.readText()).trim()
    if (text) dest.value = text
  } catch {
    // Permission denied — user can long-press/Ctrl+V into the field instead.
  }
}

const effectiveMethod = computed(() => {
  if (sendMethod.value !== 'auto') return sendMethod.value
  const amt = amount.value || 0
  if (amt <= 0) return 'lightning'
  if (amt < 1000) return 'ecash'
  if (amt > 500000) return 'onchain'
  return 'lightning'
})

// Invoice-first lightning UX: a pasted invoice that fixes its amount locks
// the amount field (auto-filled, "set by invoice"); zero-amount invoices
// leave it editable. Clearing/leaving lightning unlocks again.
// MUST come after effectiveMethod: watch() evaluates its source getter at
// setup, and reading a const still in its temporal dead zone crashed the
// whole modal at mount ("Cannot access 'R' before initialization").
const pastedInvoiceAmount = computed<number | null>(() => {
  if (effectiveMethod.value !== 'lightning') return null
  const d = dest.value.trim()
  if (!d) return null
  return parseBolt11AmountSats(d.toLowerCase().startsWith('lightning:') ? d.slice(10) : d)
})
watch(pastedInvoiceAmount, (fixed, prev) => {
  if (fixed !== null) amount.value = fixed
  // Swapping a fixed-amount invoice for a zero-amount one: don't silently
  // keep the previous invoice's sats — make the user type the new amount.
  else if (prev !== null) amount.value = 0
})

// --- Second-step confirmation (parity with the scan flow): review shows the
// --- balance reduction before anything is sent or any token is minted.

const confirming = ref(false)
const confirmBalance = ref<number | null>(null)
const invoiceAmountSats = ref<number | null>(null)

const methodLabel = computed(() => ({
  auto: 'Auto', lightning: 'Lightning', onchain: 'On-chain',
  ecash: 'Cashu', fedimint: 'Fedimint', ark: 'Ark',
}[effectiveMethod.value]))

const methodColor = computed(() => ({
  auto: 'text-white/80', lightning: 'text-yellow-400', onchain: 'text-orange-500',
  ecash: 'text-purple-400', fedimint: 'text-blue-400', ark: 'text-teal-400',
}[effectiveMethod.value]))

const destDisplay = computed(() => {
  const d = dest.value.trim()
  return d.length > 64 ? `${d.slice(0, 38)}…${d.slice(-18)}` : d
})

/** Amount encoded in a BOLT11 invoice's human-readable part, in sats (null = zero-amount). */
function parseBolt11AmountSats(invoice: string): number | null {
  const m = /^ln(?:bcrt|bc|tb)(\d+)?([munp])?1/.exec(invoice.toLowerCase())
  if (!m || !m[1]) return null
  const value = Number(m[1])
  const mult = { m: 1e-3, u: 1e-6, n: 1e-9, p: 1e-12 }[m[2] as 'm' | 'u' | 'n' | 'p'] ?? 1
  return Math.round(value * mult * 1e8)
}

// An invoice-fixed amount always wins over the typed one; a sweep is priced
// at the whole balance.
const confirmAmount = computed(() => {
  if (isSweep.value) return confirmBalance.value ?? 0
  if (effectiveMethod.value === 'lightning' && invoiceAmountSats.value !== null) return invoiceAmountSats.value
  return amount.value > 0 ? Math.floor(amount.value) : 0
})
const balanceAfter = computed(() => (confirmBalance.value ?? 0) - confirmAmount.value)
const insufficient = computed(() =>
  confirmBalance.value !== null && confirmAmount.value > confirmBalance.value && !isSweep.value
)

async function loadConfirmBalance() {
  confirmBalance.value = null
  try {
    if (effectiveMethod.value === 'ecash') {
      const res = await rpcClient.call<{ balance_sats: number }>({ method: 'wallet.ecash-balance' })
      confirmBalance.value = res.balance_sats ?? 0
    } else if (effectiveMethod.value === 'fedimint') {
      const res = await rpcClient.call<{ balance_sats: number }>({ method: 'wallet.fedimint-balance' })
      confirmBalance.value = res.balance_sats ?? 0
    } else {
      const res = await rpcClient.call<{ balance_sats: number; channel_balance_sats: number }>({ method: 'lnd.getinfo' })
      confirmBalance.value = effectiveMethod.value === 'onchain'
        ? (res.balance_sats ?? 0)
        : (res.channel_balance_sats ?? 0)
    }
  } catch {
    confirmBalance.value = null // balance preview is best-effort; send still guarded server-side
  }
}

async function review() {
  error.value = ''
  const method = effectiveMethod.value
  const d = dest.value.trim()
  if (method === 'lightning') {
    // Gate BEFORE the confirm step, not at submit: walking a user through
    // review-and-confirm only to fail on a missing node is the defect.
    if (!(await lightning.requireLightningReady('send'))) return
    if (!d) { error.value = t('web5.pasteInvoice'); return }
    invoiceAmountSats.value = parseBolt11AmountSats(d)
  } else {
    invoiceAmountSats.value = null
    if (method === 'ark' && !d) { error.value = 'Enter an Ark address, invoice or lightning address'; return }
    if (method === 'onchain' && !d) { error.value = t('web5.enterBitcoinAddress'); return }
  }
  if (!isSweep.value && confirmAmount.value <= 0 && invoiceAmountSats.value === null) {
    error.value = t('sendBitcoin.amountSats'); return
  }
  if (method === 'onchain') {
    const fee = onchainFeeParams()
    if (!fee) return
    resolvedFeeParams.value = fee
    void loadFeeEstimate()
  } else {
    resolvedFeeParams.value = {}
    feeEstimate.value = null
  }
  void loadConfirmBalance()
  confirming.value = true
}

function close() {
  error.value = ''
  ecashToken.value = ''
  confirming.value = false
  successInfo.value = null
  emit('close')
}

/** Reset the form for a fresh payment straight from the success screen. */
function sendAnother() {
  successInfo.value = null
  confirming.value = false
  dest.value = ''
  amount.value = 0
  sendAll.value = false
  error.value = ''
}

function copyText(text: string) {
  navigator.clipboard.writeText(text).catch(() => {})
}

const tokenQrCanvas = ref<HTMLCanvasElement | null>(null)
watch(ecashToken, async (token) => {
  if (!token) return
  await nextTick()
  if (!tokenQrCanvas.value) return
  try {
    const QRCode = await import('qrcode')
    await QRCode.toCanvas(tokenQrCanvas.value, token, { width: 220, margin: 1 })
  } catch { /* QR is a convenience — the copyable text is authoritative */ }
})

async function send() {
  if (processing.value) return
  // Zero typed amount is fine when the invoice fixes the amount or we sweep.
  if (!amount.value && !isSweep.value && invoiceAmountSats.value === null) return
  processing.value = true
  error.value = ''
  ecashToken.value = ''

  const method = effectiveMethod.value
  const paidAmount = confirmAmount.value
  try {
    if (method === 'ark') {
      if (!dest.value.trim()) { error.value = 'Enter an Ark address, invoice or lightning address'; return }
      await rpcClient.call<{ sent: boolean }>({
        method: 'wallet.ark-send',
        params: { destination: dest.value.trim(), amount_sats: amount.value },
        // Ark sends can wait on round participation.
        timeout: 130000,
      })
      successInfo.value = { amount: paidAmount, methodLabel: 'Sent via Ark', note: 'The transfer settles with the next Ark round.' }
    } else if (method === 'ecash') {
      const res = await rpcClient.call<{ token: string }>({
        method: 'wallet.ecash-send',
        params: { amount_sats: amount.value },
      })
      ecashToken.value = res.token
    } else if (method === 'fedimint') {
      const res = await rpcClient.call<{ token: string }>({
        method: 'wallet.fedimint-send',
        params: { amount_sats: amount.value },
        timeout: 60000,
      })
      ecashToken.value = res.token
    } else if (method === 'lightning') {
      if (!dest.value.trim()) { error.value = t('web5.pasteInvoice'); return }
      // Waits out slow multi-hop routing and only reports failure when LND
      // itself declares the payment failed — never on a timeout. The moment
      // the payment goes pending (~8s), the success pane appears in
      // "settling…" mode instead of freezing the modal through the poll —
      // the poll keeps running and upgrades the pane when LND settles.
      const res = await rpcClient.payLightningInvoice(
        { payment_request: dest.value.trim() },
        (hash) => {
          successInfo.value = {
            amount: paidAmount,
            methodLabel: 'Settling…',
            hash,
            note: 'Payment is on its way through the network. This pane updates the moment it settles — safe to close.',
          }
          confirming.value = false
          processing.value = false
          emit('sent')
        },
      )
      if (res.status === 'failed') {
        // If the settling pane is up, replace it with the failure — LND
        // declared this payment failed for real.
        successInfo.value = null
        error.value = res.failure_reason || t('web5.sendFailed')
        return
      }
      successInfo.value = {
        amount: paidAmount,
        methodLabel: res.status === 'pending' ? 'Payment in flight' : 'Paid over Lightning',
        hash: res.payment_hash || undefined,
        ...(res.status === 'pending'
          ? { note: 'This payment is taking longer than usual to settle. It will appear in your transactions once it completes.' }
          : {}),
      }
    } else {
      if (!dest.value.trim()) { error.value = t('web5.enterBitcoinAddress'); return }
      const res = await rpcClient.call<{ txid: string }>({
        method: 'lnd.sendcoins',
        params: isSweep.value
          ? { addr: dest.value.trim(), send_all: true, ...resolvedFeeParams.value }
          : { addr: dest.value.trim(), amount: amount.value, ...resolvedFeeParams.value },
      })
      successInfo.value = {
        amount: paidAmount,
        methodLabel: isSweep.value ? 'Swept on-chain' : 'Sent on-chain',
        txid: res.txid,
        note: 'On-chain payments confirm over the next blocks.',
      }
    }
    emit('sent')
    // Success pane (or the token pane for ecash mints) takes over the modal.
    confirming.value = false
  } catch (err: unknown) {
    // Running node with nothing to pay with -> funding modal, not a raw string.
    if (effectiveMethod.value === 'lightning' && lightning.handleLightningFailure(err)) return
    error.value = err instanceof Error ? err.message : t('web5.sendFailed')
  } finally {
    processing.value = false
  }
}
</script>

<style scoped>
/* Success badge (FED-06) — the branded ScreensaverRing carries the motion,
   with the emerald pop-in check centred over it. */
.send-success-badge {
  position: relative;
  width: 160px;
  height: 160px;
  display: flex;
  align-items: center;
  justify-content: center;
}
@media (min-width: 768px) {
  .send-success-badge {
    width: 192px;
    height: 192px;
  }
}
.send-success-burst {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 7rem;
  height: 7rem;
}
.burst-core {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: rgba(16, 185, 129, 0.12);
  box-shadow: 0 0 48px rgba(16, 185, 129, 0.3);
  animation: burst-pop 0.5s cubic-bezier(0.175, 0.885, 0.32, 1.4) both;
}
.burst-check {
  stroke-dasharray: 32;
  stroke-dashoffset: 32;
  animation: burst-draw 0.45s ease-out 0.25s forwards;
}
@keyframes burst-pop {
  from { transform: scale(0.3); opacity: 0; }
  to { transform: scale(1); opacity: 1; }
}
@keyframes burst-draw {
  to { stroke-dashoffset: 0; }
}
@media (prefers-reduced-motion: reduce) {
  .burst-core, .burst-check { animation: none; }
  .burst-check { stroke-dashoffset: 0; }
}
</style>
