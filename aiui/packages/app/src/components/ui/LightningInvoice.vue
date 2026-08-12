<template>
  <div
    class="rounded-xl p-4 space-y-3"
    :class="isDark
      ? 'bg-white/[0.03] border border-white/5'
      : 'bg-black/[0.02] border border-black/5'"
  >
    <!-- Header -->
    <div class="flex items-center gap-2">
      <svg class="w-5 h-5 text-[#F7931A]" viewBox="0 0 24 24" fill="currentColor">
        <path d="M13 3L4 14h7l-2 7 9-11h-7l2-7z" />
      </svg>
      <span
        class="text-sm font-semibold"
        :class="isDark ? 'text-white/90' : 'text-gray-900'"
      >
        Lightning Invoice
      </span>
      <span
        v-if="parsedAmount"
        class="ml-auto text-sm font-bold text-[#F7931A]"
      >
        {{ formattedAmount }}
      </span>
    </div>

    <!-- QR Code canvas -->
    <div class="flex justify-center">
      <canvas
        ref="qrCanvas"
        class="rounded-lg"
        :width="qrSize"
        :height="qrSize"
      />
    </div>

    <!-- Invoice string (truncated) -->
    <div
      class="text-xs font-mono break-all leading-relaxed"
      :class="isDark ? 'text-white/30' : 'text-gray-400'"
    >
      {{ truncatedInvoice }}
    </div>

    <!-- Expiry countdown -->
    <div
      v-if="expiresIn !== null"
      class="text-xs text-center"
      :class="expiresIn > 60 ? (isDark ? 'text-white/40' : 'text-gray-500') : 'text-red-400'"
    >
      {{ expiresIn > 0 ? `Expires in ${formatExpiry(expiresIn)}` : 'Expired' }}
    </div>

    <!-- Actions -->
    <div class="flex gap-2">
      <button
        class="flex-1 px-3 py-2 rounded-lg text-xs font-medium transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/70 hover:bg-white/10'
          : 'bg-gray-100 text-gray-700 hover:bg-gray-200'"
        @click="copyInvoice"
      >
        {{ copied ? 'Copied!' : 'Copy Invoice' }}
      </button>
      <button
        class="flex-1 px-3 py-2 rounded-lg text-xs font-medium transition-colors bg-gradient-to-r from-[#F7931A] to-[#E8850F] text-white hover:brightness-110"
        @click="openInWallet"
      >
        Open in Wallet
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { parseBolt11, createLightningUri, formatSats } from '@/utils/lightning'

const { isDark } = useTheme()

const props = defineProps<{
  invoice: string
  expirySeconds?: number
}>()

const qrCanvas = ref<HTMLCanvasElement | null>(null)
const copied = ref(false)
const expiresIn = ref<number | null>(props.expirySeconds ?? null)
const qrSize = 200

let expiryTimer: ReturnType<typeof setInterval> | null = null

const parsed = computed(() => parseBolt11(props.invoice))
const parsedAmount = computed(() => parsed.value.amount)
const formattedAmount = computed(() => {
  if (!parsedAmount.value) return ''
  return formatSats(parsedAmount.value)
})

const truncatedInvoice = computed(() => {
  const inv = props.invoice
  if (inv.length <= 40) return inv
  return inv.slice(0, 20) + '...' + inv.slice(-16)
})

function formatExpiry(seconds: number): string {
  if (seconds >= 3600) {
    const h = Math.floor(seconds / 3600)
    const m = Math.floor((seconds % 3600) / 60)
    return `${h}h ${m}m`
  }
  if (seconds >= 60) {
    const m = Math.floor(seconds / 60)
    const s = seconds % 60
    return `${m}m ${s}s`
  }
  return `${seconds}s`
}

function drawQR() {
  const canvas = qrCanvas.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  // Simple visual QR placeholder using invoice data hash
  // In production, use a proper QR library like 'qrcode'
  ctx.fillStyle = '#ffffff'
  ctx.fillRect(0, 0, qrSize, qrSize)

  const moduleSize = 4
  const data = props.invoice
  const modules = Math.floor(qrSize / moduleSize)

  ctx.fillStyle = '#000000'
  // Generate deterministic pattern from invoice string
  for (let y = 0; y < modules; y++) {
    for (let x = 0; x < modules; x++) {
      const charIdx = (y * modules + x) % data.length
      const val = data.charCodeAt(charIdx)
      // Position detection patterns (corners)
      const inCorner =
        (x < 7 && y < 7) ||
        (x >= modules - 7 && y < 7) ||
        (x < 7 && y >= modules - 7)
      if (inCorner) {
        const cx = x < 7 ? x : x - (modules - 7)
        const cy = y < 7 ? y : y - (modules - 7)
        const isOuter = cx === 0 || cx === 6 || cy === 0 || cy === 6
        const isInner = cx >= 2 && cx <= 4 && cy >= 2 && cy <= 4
        if (isOuter || isInner) {
          ctx.fillRect(x * moduleSize, y * moduleSize, moduleSize, moduleSize)
        }
      } else if (val % 3 !== 0) {
        ctx.fillRect(x * moduleSize, y * moduleSize, moduleSize, moduleSize)
      }
    }
  }
}

async function copyInvoice() {
  await navigator.clipboard.writeText(props.invoice)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

function openInWallet() {
  const uri = createLightningUri(props.invoice)
  window.open(uri, '_blank')
}

onMounted(() => {
  drawQR()
  if (expiresIn.value !== null && expiresIn.value > 0) {
    expiryTimer = setInterval(() => {
      if (expiresIn.value !== null && expiresIn.value > 0) {
        expiresIn.value--
      } else if (expiryTimer) {
        clearInterval(expiryTimer)
      }
    }, 1000)
  }
})

onUnmounted(() => {
  if (expiryTimer) clearInterval(expiryTimer)
})
</script>
