<template>
  <div class="rounded-xl bg-white/5 border border-white/10 p-3 space-y-2">
    <div class="flex items-center gap-2">
      <span class="text-xs text-accent/60 uppercase tracking-wider font-bold">Bitcoin Address</span>
      <span class="text-xs px-1.5 py-0.5 rounded bg-white/5 text-white/30">{{ addressType }}</span>
    </div>

    <!-- Address display -->
    <p class="text-xs font-mono text-white/70 break-all leading-relaxed select-all">{{ address }}</p>

    <!-- QR code -->
    <div class="flex justify-center py-2">
      <canvas ref="qrCanvas" class="rounded-lg" width="160" height="160" />
    </div>

    <!-- Actions -->
    <div class="flex gap-2">
      <button
        class="flex-1 py-2 rounded-lg text-xs bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
        @click="copyAddress"
      >
        {{ copied ? 'Copied!' : 'Copy Address' }}
      </button>
      <a
        :href="mempoolLink"
        target="_blank"
        rel="noopener noreferrer"
        class="flex-1 py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
      >
        View on Mempool
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'

const props = defineProps<{
  address: string
}>()

const qrCanvas = ref<HTMLCanvasElement | null>(null)
const copied = ref(false)

const addressType = computed(() => {
  const addr = props.address
  if (addr.startsWith('bc1q')) return 'SegWit (bech32)'
  if (addr.startsWith('bc1p')) return 'Taproot (bech32m)'
  if (addr.startsWith('1')) return 'Legacy (P2PKH)'
  if (addr.startsWith('3')) return 'Nested SegWit (P2SH)'
  if (addr.startsWith('tb1') || addr.startsWith('2') || addr.startsWith('m') || addr.startsWith('n')) return 'Testnet'
  return 'Unknown'
})

const mempoolLink = computed(() => {
  return `https://mempool.space/address/${props.address}`
})

function copyAddress() {
  navigator.clipboard.writeText(props.address)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

function drawQR() {
  const canvas = qrCanvas.value
  if (!canvas) return

  const ctx = canvas.getContext('2d')
  if (!ctx) return

  // Simple QR placeholder — draw bitcoin URI in styled grid
  ctx.fillStyle = '#1a1a1a'
  ctx.fillRect(0, 0, 160, 160)

  ctx.fillStyle = '#F7931A'
  ctx.font = '8px monospace'
  ctx.textAlign = 'center'

  const uri = `bitcoin:${props.address}`
  const lines: string[] = []
  for (let i = 0; i < uri.length; i += 24) {
    lines.push(uri.slice(i, i + 24))
  }
  const startY = Math.max(10, 80 - (lines.length * 5))
  lines.forEach((line, i) => {
    ctx.fillText(line, 80, startY + i * 10)
  })
}

onMounted(() => {
  drawQR()
})
</script>
