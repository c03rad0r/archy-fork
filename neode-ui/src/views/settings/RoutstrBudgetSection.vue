<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'

// Routstr prepaid budget (D-05): the assistant may only spend against an
// operator-set sats ceiling. Zero allowance = Routstr never selected. This
// panel is the ONLY UI for that ceiling; it fronts assistant.budget-get/set.
const loaded = ref(false)
const allowance = ref(0)
const spent = ref(0)
const remaining = ref(0)
const draft = ref('')
const applying = ref(false)
const error = ref('')
const savedTick = ref(false)

const active = computed(() => allowance.value > 0)

async function refresh() {
  const res = await rpcClient.call<{ allowance_sats: number; spent_sats: number; remaining_sats: number }>({
    method: 'assistant.budget-get',
  })
  allowance.value = res.allowance_sats
  spent.value = res.spent_sats
  remaining.value = res.remaining_sats
  draft.value = String(res.allowance_sats)
}

onMounted(async () => {
  try {
    await refresh()
    loaded.value = true
  } catch { /* backend without the RPC — section hides */ }
})

async function apply() {
  const parsed = Number(draft.value)
  if (!Number.isInteger(parsed) || parsed < 0) {
    error.value = 'Enter a whole number of sats (0 disables Routstr).'
    return
  }
  applying.value = true
  error.value = ''
  try {
    await rpcClient.call({ method: 'assistant.budget-set', params: { allowance_sats: parsed } })
    await refresh()
    savedTick.value = true
    setTimeout(() => { savedTick.value = false }, 2500)
  } catch (e: unknown) {
    error.value = e instanceof Error ? e.message : 'Failed to save budget'
  } finally {
    applying.value = false
  }
}
</script>

<template>
  <div v-if="loaded" class="glass-card px-6 py-6 mb-6">
    <div class="flex items-center gap-3 mb-2">
      <h2 class="text-xl font-semibold text-white/96">Routstr AI Budget</h2>
      <span
        class="text-xs font-mono px-2 py-0.5 rounded"
        :class="active ? 'bg-emerald-500/20 text-emerald-300' : 'bg-white/10 text-white/50'"
      >{{ active ? 'enabled' : 'off' }}</span>
    </div>
    <p class="text-sm text-white/60 mb-5">
      Routstr is pay-per-use AI inference, paid in sats over Cashu, used when your local
      model can't take a request. It never spends without a prepaid ceiling you set here —
      at <span class="font-mono">0</span> it is completely disabled. The assistant stops
      when the ceiling is reached; raising it widens the remainder without erasing the
      spend history.
    </p>

    <div class="grid grid-cols-3 gap-4 mb-5 text-center">
      <div class="rounded-lg bg-white/5 py-3">
        <div class="text-lg font-mono text-white/90">{{ allowance.toLocaleString() }}</div>
        <div class="text-xs text-white/50 mt-1">Allowance (sats)</div>
      </div>
      <div class="rounded-lg bg-white/5 py-3">
        <div class="text-lg font-mono text-white/90">{{ spent.toLocaleString() }}</div>
        <div class="text-xs text-white/50 mt-1">Spent</div>
      </div>
      <div class="rounded-lg bg-white/5 py-3">
        <div class="text-lg font-mono" :class="remaining > 0 ? 'text-emerald-300' : 'text-white/60'">{{ remaining.toLocaleString() }}</div>
        <div class="text-xs text-white/50 mt-1">Remaining</div>
      </div>
    </div>

    <div class="flex items-end gap-3">
      <div class="flex-1 max-w-xs">
        <label class="block text-sm text-white/60 mb-1" for="routstr-allowance">Set allowance (sats)</label>
        <input
          id="routstr-allowance"
          v-model="draft"
          type="number"
          min="0"
          step="1"
          inputmode="numeric"
          class="w-full bg-white/5 border border-white/10 rounded-lg px-4 py-2.5 text-sm text-white font-mono placeholder-white/30 focus:outline-none focus:border-white/30 transition-colors"
          @keydown.enter="apply"
        />
      </div>
      <button
        :disabled="applying || draft === String(allowance)"
        @click="apply"
        class="px-4 py-2 glass-button rounded-lg text-sm font-medium disabled:opacity-50"
      >{{ applying ? 'Saving…' : 'Save' }}</button>
      <span v-if="savedTick" class="text-sm text-emerald-300 pb-2">Saved</span>
    </div>
    <div v-if="error" class="mt-3 alert-error text-sm">{{ error }}</div>
  </div>
</template>
