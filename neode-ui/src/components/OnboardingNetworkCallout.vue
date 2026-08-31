<template>
  <Transition
    enter-active-class="transition duration-300 ease-out"
    enter-from-class="opacity-0 translate-y-2"
    enter-to-class="opacity-100 translate-y-0"
    leave-active-class="transition duration-200 ease-in"
    leave-from-class="opacity-100"
    leave-to-class="opacity-0"
  >
    <div
      v-if="visible"
      class="fixed bottom-5 left-1/2 -translate-x-1/2 z-40 w-[min(92vw,420px)] glass-card px-4 py-3 flex items-start gap-3 shadow-xl"
    >
      <svg class="w-5 h-5 text-white/60 shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393C6.957 3.83 17.043 3.83 22.606 9.393" />
      </svg>
      <div class="min-w-0 flex-1">
        <p class="text-sm font-medium text-white">No network connection</p>
        <p class="text-xs text-white/60 mt-0.5">This node has no cable or WiFi link yet. You can set up WiFi now — it also works without internet.</p>
        <div class="flex gap-2 mt-2.5">
          <button
            class="px-3 py-1.5 glass-button rounded-lg text-xs font-medium"
            @click="goToWifi"
          >
            Connect to WiFi
          </button>
          <button
            class="px-3 py-1.5 text-xs text-white/50 hover:text-white transition-colors"
            @click="dismissed = true"
          >
            Dismiss
          </button>
        </div>
      </div>
      <button class="text-white/40 hover:text-white transition-colors shrink-0" aria-label="Dismiss" @click="dismissed = true">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  </Transition>
</template>

<script lang="ts">
/** True when at least one physical interface is up (ethernet or WiFi).
 * Exported for tests — the component only needs this one pure decision. */
export function hasPhysicalLink(interfaces: { type: string; state: string }[]): boolean {
  return interfaces.some(
    (iface) => (iface.type === 'ethernet' || iface.type === 'wifi') && iface.state === 'up',
  )
}
</script>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'

/**
 * Onboarding-only "no network at all" callout (#145).
 *
 * A fresh install without a cable can leave a user stranded: the WiFi
 * settings live in Server → Network and nothing points there. This floats
 * over the onboarding steps whenever the node has NO physical link (no
 * ethernet up, no WiFi associated) and deep-links to the WiFi picker.
 *
 * Deliberately scoped the other way too: Archipelago is offline-first, so
 * "no internet" must NEVER nag — only "no link at all" qualifies, and the
 * callout is onboarding-context only (the wrapper renders it on
 * /onboarding/* routes; logged-in users have their own places to look).
 */
const router = useRouter()

const dismissed = ref(false)
const hasLink = ref<boolean | null>(null)

const visible = computed(() => !dismissed.value && hasLink.value === false)

let timer: ReturnType<typeof setInterval> | null = null
let inFlight = false

async function check() {
  if (inFlight) return
  inFlight = true
  try {
    const res = await rpcClient.call<{ interfaces: { type: string; state: string }[] }>({
      method: 'network.list-interfaces',
      dedup: true,
      maxRetries: 1,
    })
    hasLink.value = hasPhysicalLink(res?.interfaces ?? [])
  } catch {
    // Node busy or RPC not ready during early onboarding — never nag on a
    // failed probe; treat unknown as "don't show".
    hasLink.value = null
  } finally {
    inFlight = false
  }
}

function goToWifi() {
  dismissed.value = true
  // Server.vue consumes ?open=wifi by popping the WiFi picker on arrival.
  router.push('/dashboard/server?open=wifi')
}

onMounted(() => {
  check()
  // A cable gets plugged in mid-onboarding; poll gently so the callout
  // dismisses itself the moment a link exists.
  timer = setInterval(check, 15_000)
})

onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>
