<template>
  <div class="rounded-xl bg-white/5 border border-white/10 p-3 space-y-2">
    <div class="flex items-center gap-2">
      <span class="text-xs text-accent/60 uppercase tracking-wider font-bold">Fedimint Ecash</span>
    </div>

    <p class="text-xs font-mono text-white/50 break-all line-clamp-2 select-all">{{ token }}</p>

    <div class="flex items-center gap-3">
      <span class="text-sm font-bold text-accent tabular-nums">{{ formattedAmount }}</span>
    </div>

    <div class="flex gap-2">
      <button
        class="flex-1 py-2 rounded-lg text-xs bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
        @click="copyToken"
      >
        {{ copied ? 'Copied!' : 'Copy Token' }}
      </button>
      <a
        :href="fediLink"
        class="flex-1 py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
      >
        Receive in Fedi
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'

const props = defineProps<{
  token: string
}>()

const copied = ref(false)

const formattedAmount = computed(() => {
  // Fedimint ecash tokens are base64-encoded — we can't decode amount without
  // the Fedimint library, so show a generic label
  return 'ecash token'
})

const fediLink = computed(() => {
  return `fedi://receive?token=${encodeURIComponent(props.token)}`
})

function copyToken() {
  navigator.clipboard.writeText(props.token)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}
</script>
