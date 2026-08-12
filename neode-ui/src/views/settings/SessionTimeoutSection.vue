<script setup lang="ts">
/**
 * How long a login lasts on this node.
 *
 * Presented as two plain questions — "sign me out after quiet" and "always
 * sign me out after" — rather than as the two-token mechanism underneath.
 * The distinction that matters to the operator is that the second one is
 * what actually guarantees a login ends: this dashboard polls constantly,
 * so an idle timeout alone never fires on an open tab.
 */
import { ref, onMounted, computed } from 'vue'
import { rpcClient } from '@/api/rpc-client'

type Policy = {
  idle_timeout_secs: number
  absolute_timeout_secs: number | null
  reauth_for_funds: boolean
}

const idle = ref<number>(86400)
const absolute = ref<number | null>(30 * 24 * 3600)
const reauthForFunds = ref(true)
const loading = ref(true)
const saving = ref(false)
const saved = ref(false)
const error = ref('')

// Offered as presets rather than a free number field: the useful values are
// few, and a box accepting "5" invites someone to lock themselves out.
const idleChoices = [
  { label: '15 minutes', value: 15 * 60 },
  { label: '1 hour', value: 3600 },
  { label: '1 day', value: 86400 },
  { label: '1 week', value: 7 * 24 * 3600 },
  { label: '30 days', value: 30 * 24 * 3600 },
]
const absoluteChoices = [
  { label: '1 day', value: 86400 },
  { label: '1 week', value: 7 * 24 * 3600 },
  { label: '30 days', value: 30 * 24 * 3600 },
  { label: '90 days', value: 90 * 24 * 3600 },
  { label: 'Never', value: null },
]

const shortIdleWarning = computed(() => idle.value <= 3600)

async function load() {
  loading.value = true
  try {
    const p = await rpcClient.call<Policy>({ method: 'auth.session-policy.get', params: {} })
    idle.value = p.idle_timeout_secs
    absolute.value = p.absolute_timeout_secs
    reauthForFunds.value = p.reauth_for_funds
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function save() {
  saving.value = true
  saved.value = false
  error.value = ''
  try {
    // The backend clamps and returns what it stored, so reflect that back
    // rather than assuming our values were taken verbatim.
    const p = await rpcClient.call<Policy>({
      method: 'auth.session-policy.set',
      params: {
        idle_timeout_secs: idle.value,
        absolute_timeout_secs: absolute.value,
        reauth_for_funds: reauthForFunds.value,
      },
    })
    idle.value = p.idle_timeout_secs
    absolute.value = p.absolute_timeout_secs
    reauthForFunds.value = p.reauth_for_funds
    saved.value = true
    setTimeout(() => { saved.value = false }, 2500)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}

onMounted(load)
</script>

<template>
  <div class="mb-6">
    <h3 class="text-base font-medium text-white/90 mb-1">Session timeout</h3>
    <p class="text-sm text-white/60 mb-4">
      How long this node keeps you signed in. TV and kiosk screens are never
      signed out for sitting idle — there is nobody there to sign them back in.
    </p>

    <div v-if="error" role="alert" class="mb-4 p-3 bg-red-500/20 border border-red-500/40 rounded-lg text-red-200 text-sm">
      {{ error }}
    </div>

    <div v-if="!loading" class="space-y-4">
      <div>
        <label for="idle-timeout" class="block text-sm font-medium text-white/80 mb-2">Sign me out after this much inactivity</label>
        <select
          id="idle-timeout"
          v-model.number="idle"
          class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40"
        >
          <option v-for="c in idleChoices" :key="c.value" :value="c.value" class="bg-neutral-900">{{ c.label }}</option>
        </select>
        <p v-if="shortIdleWarning" class="text-xs text-orange-300/80 mt-2">
          Short timeouts are what payment-industry rules ask for when funds are involved — expect to sign in often.
        </p>
      </div>

      <div>
        <label for="absolute-timeout" class="block text-sm font-medium text-white/80 mb-2">Always sign me out after</label>
        <select
          id="absolute-timeout"
          v-model="absolute"
          class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white focus:outline-none focus:border-white/40"
        >
          <option v-for="c in absoluteChoices" :key="String(c.value)" :value="c.value" class="bg-neutral-900">{{ c.label }}</option>
        </select>
        <p class="text-xs text-white/50 mt-2">
          Counts from when you signed in, whatever you are doing. This is the one that
          guarantees a session ends: an open dashboard is never idle, so the setting
          above would not fire on it.
        </p>
      </div>

      <label class="flex items-start gap-3 cursor-pointer">
        <input v-model="reauthForFunds" type="checkbox" class="mt-1 accent-orange-500" />
        <span class="text-sm text-white/80">
          Ask for my password again before sending funds
          <span class="block text-xs text-white/50">Recommended. Applies however recently you signed in.</span>
        </span>
      </label>

      <button
        :disabled="saving"
        class="w-full glass-button px-6 py-3 rounded-lg font-medium transition-all hover:bg-black/70 disabled:opacity-50"
        @click="save"
      >
        {{ saving ? 'Saving…' : (saved ? 'Saved' : 'Save session settings') }}
      </button>
    </div>
  </div>
</template>
