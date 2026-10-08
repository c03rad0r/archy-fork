<template>
  <BaseModal :show="show && !funding" title="Connect your AI" max-width="max-w-xl" @close="close">
    <p class="text-sm text-white/60 mb-4">Choose how your assistant connects. API keys stay on this node and never enter your chat.</p>
    <p v-if="loading" role="status" class="text-sm text-white/60 mb-3">Checking this node…</p>
    <p v-if="error" role="alert" class="text-sm text-amber-200 mb-3">{{ error }}</p>
    <div class="grid grid-cols-1 sm:grid-cols-3 gap-2 mb-4">
      <button v-for="choice in choices" :key="choice.id" class="glass-button rounded-lg px-3 py-3 text-sm text-white" :aria-pressed="mode === choice.id" @click="choose(choice.id)">{{ choice.label }}</button>
    </div>
    <form v-if="mode === 'claude' || mode === 'openai'" class="space-y-3" @submit.prevent="save">
      <p class="text-xs text-white/60">{{ mode === 'openai' ? 'Use an OpenAI API key with API billing. Choose a chat model your project can access.' : 'Use your Anthropic API key. Usage is billed to that API account.' }}</p>
      <p v-if="configured" class="text-sm text-white/70">A key is already configured. Leave this field empty to keep it.</p>
      <label class="block text-sm text-white/80" for="ai-connection-key">{{ mode === 'openai' ? 'OpenAI' : 'Claude' }} API key</label>
      <input id="ai-connection-key" v-model="key" type="password" autocomplete="off" spellcheck="false" class="input-glass w-full" placeholder="Paste API key" :disabled="saving" />
      <template v-if="mode === 'openai'">
        <label class="block text-sm text-white/80" for="ai-connection-model">Model ID</label>
        <input id="ai-connection-model" v-model="model" type="text" spellcheck="false" class="input-glass w-full" placeholder="Enter your OpenAI model ID" :disabled="saving" />
      </template>
      <button class="glass-button rounded-lg px-4 py-2 text-sm text-white w-full" :disabled="saving || loading || (!key.trim() && !configured) || (mode === 'openai' && !model.trim())">{{ saving ? 'Saving…' : 'Save and continue' }}</button>
    </form>
    <div v-else-if="mode === 'routstr'" class="space-y-3">
      <p class="text-sm text-white/70">Pay for AI with ecash. Add funds to this node’s wallet and set the most it may spend.</p>
      <p class="text-sm text-white/70">Ecash balance: {{ balance === null ? 'Unavailable' : balance.toLocaleString() + ' sats' }}</p>
      <button class="glass-button rounded-lg px-4 py-2 text-sm text-white" @click="funding = true">Add ecash</button>
      <RoutstrBudgetSection />
      <button class="glass-button rounded-lg px-4 py-2 text-sm text-white w-full" @click="continueRoutstr">Use Routstr</button>
      <p class="text-xs text-white/50">The selected model’s price and accepted mint still apply. No payment is sent by opening this setup.</p>
    </div>
    <button v-if="status?.local_ready" class="glass-button rounded-lg px-4 py-2 text-sm text-white mt-4 w-full" @click="useLocal">Use local AI</button>
    <p class="text-xs text-white/40 mt-4">Your draft stays in place when you close this window.</p>
  </BaseModal>
  <ReceiveBitcoinModal :show="show && funding" initial-method="ecash" @close="finishFunding" @received="finishFunding" />
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import BaseModal from './BaseModal.vue'
import ReceiveBitcoinModal from './ReceiveBitcoinModal.vue'
import RoutstrBudgetSection from '@/views/settings/RoutstrBudgetSection.vue'

type Choice = 'claude' | 'openai' | 'routstr'
interface ProviderStatus {
  schema: number
  settings: { provider: 'auto' | 'local' | 'routstr' | 'claude' | 'openai'; openai_model: string }
  claude_configured: boolean
  openai_configured: boolean
  local_ready: boolean | null
  routstr_remaining_sats: number
}
const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: []; configured: [provider: Choice | 'auto' | 'local', model?: string] }>()
const choices: { id: Choice; label: string }[] = [{ id: 'claude', label: 'Claude API' }, { id: 'openai', label: 'OpenAI API' }, { id: 'routstr', label: 'Routstr · sats' }]
const mode = ref<Choice | null>(null)
const key = ref('')
const model = ref('')
const status = ref<ProviderStatus | null>(null)
const balance = ref<number | null>(null)
const error = ref('')
const loading = ref(false)
const saving = ref(false)
const funding = ref(false)
const configured = computed(() => mode.value === 'claude' ? status.value?.claude_configured : status.value?.openai_configured)
let refreshPromise: Promise<ProviderStatus | null> | null = null
async function refresh(): Promise<ProviderStatus | null> {
  if (refreshPromise) return refreshPromise
  loading.value = true
  refreshPromise = (async () => {
    try {
      const response = await rpcClient.call<{ value: ProviderStatus | null }>({ method: 'system.settings.get', params: { key: 'ai_provider_status' }, timeout: 8000 })
      if (response.value?.schema !== 1) throw new Error('AI connection setup needs the matching node update.')
      status.value = response.value
      if (!model.value) model.value = response.value.settings.openai_model || ''
      return response.value
    } catch {
      error.value = 'Could not check AI connections. Try again when this node is reachable.'
      return null
    } finally { loading.value = false; refreshPromise = null }
  })()
  return refreshPromise
}
async function checkNeeded(): Promise<boolean> {
  const state = await refresh()
  if (!state) return false // An unavailable status endpoint is not proof of missing configuration.
  if (state.settings.provider === 'openai') return !state.openai_configured
  if (state.settings.provider === 'claude') return !state.claude_configured
  if (state.settings.provider === 'local') return state.local_ready === false
  if (state.settings.provider === 'routstr' || (state.local_ready === false && !state.claude_configured)) {
    if (state.routstr_remaining_sats <= 0) return true
    await refreshBalance()
    return balance.value === 0
  }
  return false
}
async function refreshBalance() {
  try { const result = await rpcClient.call<{ balance_sats: number }>({ method: 'wallet.ecash-balance', timeout: 8000 }); balance.value = Number.isFinite(result.balance_sats) ? result.balance_sats : null }
  catch { balance.value = null }
}
function choose(choice: Choice) { key.value = ''; error.value = ''; mode.value = choice; if (choice === 'routstr') void refreshBalance() }
function close() { key.value = ''; funding.value = false; emit('close') }
async function save() {
  if ((mode.value !== 'claude' && mode.value !== 'openai') || saving.value) return
  const provider = mode.value
  saving.value = true; error.value = ''
  try {
    if (key.value.trim()) {
      await rpcClient.call({ method: 'system.settings.set', params: { key: `${provider}_api_key`, value: key.value.trim() } })
      key.value = ''
      if (status.value) status.value[provider === 'claude' ? 'claude_configured' : 'openai_configured'] = true
    } else if (!configured.value) return
    await rpcClient.call({ method: 'system.settings.set', params: { key: 'ai_provider', value: JSON.stringify({ provider, openai_model: model.value.trim() }) } })
    emit('configured', provider, provider === 'openai' ? model.value.trim() : undefined); close()
  } catch { error.value = 'Could not save the connection. Check the key and model, then retry.' }
  finally { saving.value = false }
}
async function useLocal() {
  error.value = ''
  try {
    await rpcClient.call({ method: 'system.settings.set', params: { key: 'ai_provider', value: JSON.stringify({ provider: 'local', openai_model: model.value }) } })
    emit('configured', 'local'); close()
  } catch { error.value = 'Could not select local AI. Try again.' }
}
async function continueRoutstr() {
  const [state] = await Promise.all([refresh(), refreshBalance()])
  if (!state || state.routstr_remaining_sats <= 0) { error.value = 'Set a spending allowance before using Routstr.'; return }
  if (balance.value === null || balance.value <= 0) { error.value = 'Add ecash to this node before using Routstr.'; return }
  try {
    await rpcClient.call({ method: 'system.settings.set', params: { key: 'ai_provider', value: JSON.stringify({ provider: 'routstr', openai_model: model.value }) } })
    emit('configured', 'routstr'); close()
  } catch { error.value = 'Could not select Routstr. Try again.' }
}
async function finishFunding() { funding.value = false; await refreshBalance() }
watch(() => props.show, open => { if (open) { error.value = ''; void refresh() } else { key.value = ''; funding.value = false } })
async function syncSelection() { const state = await refresh(); if (state) emit('configured', state.settings.provider, state.settings.provider === 'openai' ? state.settings.openai_model : undefined) }
defineExpose({ checkNeeded, syncSelection, showRoutstr: () => choose('routstr') })
</script>
