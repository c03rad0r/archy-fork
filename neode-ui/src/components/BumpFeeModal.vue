<template>
  <BaseModal :show="show" title="Bump transaction" max-width="max-w-md" z-index="z-[3100]" @close="close">
    <PaymentSuccessPane v-if="accepted" :amount="0"
      :verb="operation?.status === 'confirmed' ? 'CONFIRMED' : 'BUMP BROADCAST'"
      :method-label="operation?.quote?.method === 'rbf' ? 'Replace by fee · RBF' : 'Child pays for parent · CPFP'"
      :rows="successRows"
      :note="operation?.status === 'confirmed' ? 'Your fee bump is confirmed on-chain.' : 'Accepted in the node’s mempool. Awaiting confirmation; next-block inclusion is not guaranteed.'"
      :again-label="operation?.status === 'confirmed' ? '' : 'Check status'"
      @again="checkStatus" @done="close" />
    <div v-else class="space-y-4">
      <p class="text-xs font-mono text-white/45 break-all">{{ txid }}</p>
      <template v-if="!operation">
        <div class="flex gap-2" role="group" aria-label="Fee target">
          <button v-for="option in ['next', 'custom'] as const" :key="option" type="button"
            :disabled="submitting" :aria-pressed="mode === option"
            class="flex-1 rounded-lg px-3 py-2 text-sm border transition-colors"
            :class="mode === option ? 'bg-orange-500/20 border-orange-400/40 text-orange-200' : 'border-white/10 text-white/60'"
            @click="mode = option">{{ option === 'next' ? 'Next block' : 'Custom' }}</button>
        </div>
        <label v-if="mode === 'custom'" class="block text-sm text-white/70">
          Fee rate (sat/vB)
          <input v-model.number="customRate" :disabled="submitting" type="number" min="1" max="5000" step="1"
            class="mt-1 block w-full rounded-lg bg-white/5 border border-white/15 px-3 py-2 text-white" />
        </label>
        <p class="text-xs text-white/45">Targets faster confirmation. Next-block confirmation is not guaranteed.</p>
        <p v-if="loading" role="status" class="text-sm text-white/60">Checking the transaction and current fees…</p>
        <template v-if="quote">
          <div class="rounded-lg bg-white/5 p-3 space-y-2">
            <p class="text-sm font-medium text-white">{{ quote.method === 'rbf' ? 'RBF · Replace sweep' : 'CPFP · Spend change' }}</p>
            <p class="text-xs text-white/55">{{ quote.method === 'rbf'
              ? 'Replaces the wallet’s fee-bump transaction with a higher-fee version. The original payment stays unchanged.'
              : 'RBF is unavailable for this payment. A child transaction spends your change to accelerate the original payment.' }}</p>
            <dl class="text-sm space-y-2 pt-2">
              <div class="flex justify-between gap-3"><dt class="text-white/50">Recipient amount</dt><dd class="text-white">{{ sats(quote.recipient_sats) }}</dd></div>
              <div class="flex justify-between gap-3"><dt class="text-white/50">Current total fee</dt><dd class="text-white">{{ sats(quote.current_fee_sats) }}</dd></div>
              <div class="flex justify-between gap-3"><dt class="text-white/50">Additional fee · up to</dt><dd class="text-orange-200">{{ sats(quote.additional_fee_sats) }}</dd></div>
              <div class="flex justify-between gap-3"><dt class="text-white/50">New total fee · up to</dt><dd class="text-white font-medium">{{ sats(quote.total_fee_sats) }}</dd></div>
              <div class="flex justify-between gap-3"><dt class="text-white/50">Package target</dt><dd class="text-white">{{ quote.rate_sat_vb }} sat/vB</dd></div>
            </dl>
          </div>
          <p class="text-xs text-white/45">The wallet may use the full {{ sats(quote.budget_sats) }} sweep budget by the next block. The fee cap applies only to this bump.</p>
          <p v-if="expired" role="status" class="text-xs text-amber-300">Fee quote expired. Refresh before confirming.</p>
        </template>
        <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
        <div class="flex gap-2">
          <button type="button" class="flex-1 px-3 py-2 rounded-lg bg-white/10 text-white/75 text-sm" :disabled="loading || submitting" @click="getQuote">{{ quote ? 'Refresh quote' : 'Preview fee' }}</button>
          <button v-if="quote" type="button" class="flex-1 px-3 py-2 rounded-lg bg-orange-500 text-white text-sm font-medium disabled:opacity-40"
            :disabled="submitting || loading || expired" @click="submit">{{ submitting ? 'Submitting…' : 'Confirm bump' }}</button>
        </div>
      </template>
      <template v-else>
        <p role="status" class="text-sm text-white/80">{{ operation.message }}</p>
        <p v-if="operation.bump_txid" class="text-xs text-white/50 break-all">{{ operation.quote?.method === 'rbf' ? 'Replacement' : 'Child' }}: {{ operation.bump_txid }}</p>
        <p v-if="operation.actual_sweep_fee_sats !== undefined" class="text-sm text-white/70">Sweep fee: {{ sats(operation.actual_sweep_fee_sats) }}</p>
        <p v-if="error" role="alert" class="text-sm text-red-300">{{ error }}</p>
        <button v-if="operation.status !== 'confirmed'" type="button" class="w-full px-3 py-2 rounded-lg bg-white/10 text-white text-sm" :disabled="loading" @click="checkStatus">Check status</button>
      </template>
    </div>
  </BaseModal>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import BaseModal from './BaseModal.vue'
import PaymentSuccessPane, { type SuccessRow } from './PaymentSuccessPane.vue'
import { rpcClient } from '@/api/rpc-client'
interface Quote {
  quote_id: string; txid: string; expires_at: number; method: 'rbf' | 'cpfp'
  recipient_sats: number; current_fee_sats: number; additional_fee_sats: number
  total_fee_sats: number; budget_sats: number; rate_sat_vb: number
}
interface Operation {
  status: 'none' | 'registered' | 'unknown' | 'mempool' | 'confirmed'
  message: string; bump_txid?: string; actual_sweep_fee_sats?: number; quote?: Quote
}
const props = defineProps<{ show: boolean; txid: string }>()
const emit = defineEmits<{ close: []; updated: [] }>()
const mode = ref<'next' | 'custom'>('next')
const customRate = ref<number | ''>('')
const quote = ref<Quote | null>(null)
const operation = ref<Operation | null>(null)
const loading = ref(false)
const submitting = ref(false)
const error = ref('')
const time = ref(Date.now())
let generation = 0
let lastStatusCheck = 0
const timer = setInterval(() => {
  time.value = Date.now()
  if (props.show && operation.value && operation.value.status !== 'confirmed' && time.value - lastStatusCheck > 5000) void checkStatus()
}, 1000)
onUnmounted(() => { clearInterval(timer); generation++ })
const expired = computed(() => !quote.value || quote.value.expires_at * 1000 <= time.value)
const sats = (n: number) => `${n.toLocaleString()} sats`
const accepted = computed(() => operation.value?.status === 'mempool' || operation.value?.status === 'confirmed')
const successRows = computed<SuccessRow[]>(() => {
  const op = operation.value
  if (!op) return []
  const rows: SuccessRow[] = [{ label: 'Original transaction', value: props.txid }]
  if (op.bump_txid) rows.push({ label: op.quote?.method === 'rbf' ? 'Replacement transaction' : 'Child transaction', value: op.bump_txid })
  if (op.actual_sweep_fee_sats !== undefined) rows.push({ label: 'Sweep fee', value: sats(op.actual_sweep_fee_sats) })
  return rows
})
const message = (e: unknown) => e instanceof Error ? e.message : String(e)
function close() { if (!submitting.value) emit('close') }
watch([mode, customRate], () => { generation++; loading.value = false; quote.value = null; error.value = '' })
watch(() => [props.show, props.txid] as const, async ([show]) => {
  generation++; quote.value = null; operation.value = null; error.value = ''; loading.value = false
  mode.value = 'next'; customRate.value = ''
  if (show && props.txid) {
    // Let mode/reset watchers settle before the first request.
    await Promise.resolve()
    await checkStatus()
    if (!operation.value && !error.value && props.show) await getQuote()
  }
}, { immediate: true })
async function getQuote() {
  if (loading.value || submitting.value || operation.value) return
  if (mode.value === 'custom' && (!Number.isInteger(customRate.value) || Number(customRate.value) < 1 || Number(customRate.value) > 5000)) {
    error.value = 'Enter a whole-number fee rate from 1 to 5000 sat/vB.'; return
  }
  const request = ++generation
  loading.value = true; error.value = ''; quote.value = null
  try {
    const result = await rpcClient.call<Quote>({ method: 'lnd.bump-quote', params: { txid: props.txid, ...(mode.value === 'custom' ? { sat_per_vbyte: customRate.value } : {}) }, maxRetries: 1, timeout: 60000 })
    if (request === generation) quote.value = result
  } catch (e) { if (request === generation) error.value = message(e) }
  finally { if (request === generation) loading.value = false }
}
async function submit() {
  if (submitting.value || loading.value || !quote.value || expired.value) return
  submitting.value = true; error.value = ''
  const id = props.txid
  try {
    operation.value = await rpcClient.call<Operation>({ method: 'lnd.bump-submit', params: { txid: id, quote_id: quote.value.quote_id }, maxRetries: 1, timeout: 90000 })
    emit('updated')
  } catch (e) {
    // An interrupted response may conceal success. A status check must happen
    // before showing another quote/submit action.
    operation.value = { status: 'unknown', message: 'Submission response was not confirmed. Check status before taking another action.' }
    error.value = message(e)
  } finally { submitting.value = false }
  await checkStatus()
}
async function checkStatus() {
  if (loading.value || submitting.value) return
  lastStatusCheck = Date.now()
  const request = ++generation
  loading.value = true
  try {
    const result = await rpcClient.call<Operation>({ method: 'lnd.bump-status', params: { txid: props.txid }, maxRetries: 1, timeout: 60000 })
    if (request !== generation) return
    error.value = ''
    if (result.status === 'none') {
      operation.value = null
      quote.value = null
    } else {
      operation.value = result
      if (result.status === 'confirmed' || result.status === 'mempool') emit('updated')
    }
  } catch (e) { if (request === generation) error.value = message(e) }
  finally { if (request === generation) loading.value = false }
}
</script>
