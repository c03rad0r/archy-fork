<template>
  <div class="rounded-xl bg-white/5 border border-white/10 p-3 space-y-2">
    <div class="flex items-center gap-2">
      <span class="text-xs text-accent/60 uppercase tracking-wider font-bold">Lightning Invoice</span>
      <span
        v-if="isExpired"
        class="text-xs px-1.5 py-0.5 rounded bg-red-400/15 text-red-400/80"
      >
        Expired
      </span>
    </div>

    <!-- Amount -->
    <div v-if="decodedAmount" class="flex items-center gap-2">
      <span class="text-lg font-bold text-accent tabular-nums">{{ formatSats(decodedAmount) }}</span>
      <span class="text-xs text-white/30">sats</span>
    </div>

    <!-- Description -->
    <p v-if="decodedDescription" class="text-xs text-white/50">{{ decodedDescription }}</p>

    <!-- Expiry countdown -->
    <div v-if="expiryText" class="flex items-center gap-1.5">
      <span
        class="text-xs tabular-nums"
        :class="isExpired ? 'text-red-400/60' : isExpiringSoon ? 'text-red-400/60' : 'text-white/30'"
      >
        {{ expiryText }}
      </span>
    </div>

    <!-- Invoice string -->
    <p class="text-xs font-mono text-white/30 break-all line-clamp-2 select-all">{{ invoice }}</p>

    <div class="flex gap-2">
      <button
        class="flex-1 py-2 rounded-lg text-xs bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
        @click="copyInvoice"
      >
        {{ copied ? 'Copied!' : 'Copy' }}
      </button>
      <a
        :href="'lightning:' + invoice"
        class="flex-1 py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
      >
        Pay with wallet
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps<{
  invoice: string
}>()

const copied = ref(false)
const now = ref(Math.floor(Date.now() / 1000))
let timer: ReturnType<typeof setInterval> | null = null

// Basic BOLT11 decoding (amount from hrp, no full decode)
const decodedAmount = computed(() => {
  const lower = props.invoice.toLowerCase()
  // lnbc<amount><multiplier>1...
  const match = lower.match(/^lnbc(\d+)([munp]?)1/)
  if (!match) return null

  const num = parseInt(match[1])
  const mult = match[2]

  switch (mult) {
    case 'm': return num * 100000 // milli-btc to sats
    case 'u': return num * 100   // micro-btc to sats
    case 'n': return Math.round(num * 0.1) // nano-btc to sats
    case 'p': return Math.round(num * 0.0001) // pico-btc to sats
    default: return num * 100000000 // btc to sats
  }
})

const decodedDescription = computed<string | null>(() => {
  // Description is in tagged fields — basic extraction not possible without full decode
  return null
})

const expiryTimestamp = computed<number | null>(() => {
  // Default BOLT11 expiry is 3600s — we can't decode exact timestamp without full decode
  return null
})

const isExpired = computed(() => {
  if (!expiryTimestamp.value) return false
  return now.value > expiryTimestamp.value
})

const isExpiringSoon = computed(() => {
  if (!expiryTimestamp.value) return false
  return expiryTimestamp.value - now.value < 300 // < 5 min
})

const expiryText = computed(() => {
  if (!expiryTimestamp.value) return null
  const diff = expiryTimestamp.value - now.value
  if (diff <= 0) return 'Expired'
  const min = Math.floor(diff / 60)
  if (min < 60) return `Expires in ${min}m`
  return `Expires in ${Math.floor(min / 60)}h ${min % 60}m`
})

function formatSats(sats: number): string {
  return sats.toLocaleString()
}

function copyInvoice() {
  navigator.clipboard.writeText(props.invoice)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

onMounted(() => {
  timer = setInterval(() => {
    now.value = Math.floor(Date.now() / 1000)
  }, 1000)
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>
