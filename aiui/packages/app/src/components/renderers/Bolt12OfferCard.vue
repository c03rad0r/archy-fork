<template>
  <div class="rounded-xl bg-white/5 border border-white/10 p-3 space-y-2">
    <div class="flex items-center gap-2">
      <span class="text-xs text-accent/60 uppercase tracking-wider font-bold">BOLT12 Offer</span>
    </div>

    <p class="text-xs font-mono text-white/50 break-all line-clamp-2 select-all">{{ offer }}</p>

    <!-- QR -->
    <div class="flex justify-center py-2">
      <canvas ref="qrCanvas" class="rounded-lg" width="140" height="140" />
    </div>

    <div class="flex gap-2">
      <button
        class="flex-1 py-2 rounded-lg text-xs bg-white/5 text-white/50 hover:text-white/70 hover:bg-white/10 transition-colors"
        @click="copyOffer"
      >
        {{ copied ? 'Copied!' : 'Copy Offer' }}
      </button>
      <a
        :href="'lightning:' + offer"
        class="flex-1 py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
      >
        Pay with wallet
      </a>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'

const props = defineProps<{
  offer: string
}>()

const qrCanvas = ref<HTMLCanvasElement | null>(null)
const copied = ref(false)

function copyOffer() {
  navigator.clipboard.writeText(props.offer)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

function drawQR() {
  const canvas = qrCanvas.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  ctx.fillStyle = '#1a1a1a'
  ctx.fillRect(0, 0, 140, 140)
  ctx.fillStyle = '#F7931A'
  ctx.font = '7px monospace'
  ctx.textAlign = 'center'

  const lines: string[] = []
  for (let i = 0; i < props.offer.length; i += 22) {
    lines.push(props.offer.slice(i, i + 22))
  }
  const startY = Math.max(8, 70 - (lines.length * 4))
  lines.slice(0, 14).forEach((line, i) => {
    ctx.fillText(line, 70, startY + i * 9)
  })
}

onMounted(() => {
  drawQR()
})
</script>
