<template>
  <div
    v-if="parsed"
    class="rounded-xl p-3 space-y-2 my-2"
    :class="isDark
      ? 'bg-white/[0.03] border border-[#F7931A]/20'
      : 'bg-black/[0.02] border border-[#F7931A]/20'"
  >
    <!-- Header -->
    <div class="flex items-center gap-2">
      <div class="w-6 h-6 rounded-full bg-[#F7931A]/10 flex items-center justify-center">
        <svg class="w-3.5 h-3.5 text-[#F7931A]" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="12" cy="12" r="10" />
          <text x="12" y="16" text-anchor="middle" fill="white" font-size="12" font-weight="bold">C</text>
        </svg>
      </div>
      <span
        class="text-xs font-semibold"
        :class="isDark ? 'text-white/80' : 'text-gray-800'"
      >
        Cashu Token
      </span>
      <span class="ml-auto text-sm font-bold text-[#F7931A]">
        {{ formattedAmount }}
      </span>
    </div>

    <!-- Mint info -->
    <div
      class="text-xs font-mono"
      :class="isDark ? 'text-white/30' : 'text-gray-400'"
    >
      Mint: {{ displayMint }}
    </div>

    <!-- Memo -->
    <div
      v-if="parsed.memo"
      class="text-xs"
      :class="isDark ? 'text-white/50' : 'text-gray-500'"
    >
      {{ parsed.memo }}
    </div>

    <!-- Actions -->
    <div class="flex gap-2">
      <button
        class="flex-1 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/60 hover:bg-white/10'
          : 'bg-gray-100 text-gray-600 hover:bg-gray-200'"
        @click="copyToken"
      >
        {{ copied ? 'Copied!' : 'Copy Token' }}
      </button>
      <button
        class="flex-1 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors bg-[#F7931A]/10 text-[#F7931A] hover:bg-[#F7931A]/20"
        @click="openInWallet"
      >
        Open in Wallet
      </button>
    </div>
  </div>

  <!-- Fallback for unparseable tokens -->
  <div
    v-else
    class="rounded-lg p-2 my-1 text-xs font-mono break-all"
    :class="isDark ? 'bg-white/5 text-white/40' : 'bg-gray-50 text-gray-500'"
  >
    {{ truncatedRaw }}
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { parseCashuToken, formatMintUrl, formatCashuAmount } from '@/utils/cashu'

const { isDark } = useTheme()

const props = defineProps<{
  token: string
}>()

const copied = ref(false)

const parsed = computed(() => parseCashuToken(props.token))

const formattedAmount = computed(() => {
  if (!parsed.value) return ''
  return formatCashuAmount(parsed.value.amount, parsed.value.unit)
})

const displayMint = computed(() => {
  if (!parsed.value) return ''
  return formatMintUrl(parsed.value.mint)
})

const truncatedRaw = computed(() => {
  const t = props.token
  if (t.length <= 40) return t
  return t.slice(0, 20) + '...' + t.slice(-16)
})

async function copyToken() {
  await navigator.clipboard.writeText(props.token)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

function openInWallet() {
  // Use web+cashu: URI scheme for wallet deep-linking
  window.open(`web+cashu:${props.token}`, '_blank')
}
</script>
