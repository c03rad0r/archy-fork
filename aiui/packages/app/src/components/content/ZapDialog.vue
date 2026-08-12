<template>
  <div
    v-if="isOpen"
    ref="dialogRef"
    role="dialog"
    aria-modal="true"
    aria-label="Send zap"
    class="fixed inset-0 z-50 flex items-center justify-center"
    @click.self="close"
    @keydown.escape="close"
    @keydown.tab="trapFocus"
  >
    <div class="absolute inset-0 bg-black/60 backdrop-blur-sm" @click="close" />
    <div class="relative glass-card w-[320px] max-w-[90vw] p-5 space-y-4 animate-scale-in">
      <div class="flex items-center justify-between">
        <h3 class="text-sm font-bold text-white/90">Zap</h3>
        <button
          ref="closeButtonRef"
          class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded text-white/40 hover:text-white/70 transition-colors"
          aria-label="Close"
          @click="close"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      <!-- Target info -->
      <p class="text-xs text-white/40 truncate font-mono">{{ targetName }}</p>

      <!-- Amount presets -->
      <div class="flex gap-1.5 flex-wrap">
        <button
          v-for="preset in amountPresets"
          :key="preset"
          class="text-xs px-2.5 py-1.5 rounded-lg transition-colors"
          :class="amount === preset
            ? 'bg-accent/20 text-accent border border-accent/30'
            : 'bg-white/5 text-white/50 hover:bg-white/10'"
          @click="amount = preset"
        >
          {{ formatSats(preset) }}
        </button>
      </div>

      <!-- Custom amount -->
      <div>
        <label class="text-xs text-white/30 block mb-1">Amount (sats)</label>
        <input
          v-model.number="amount"
          type="number"
          min="1"
          class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors tabular-nums"
          placeholder="21"
        />
      </div>

      <!-- Optional message -->
      <div>
        <label class="text-xs text-white/30 block mb-1">Message (optional)</label>
        <input
          v-model="message"
          type="text"
          class="w-full px-3 py-2 rounded-lg text-base bg-white/5 text-white/80 placeholder:text-white/25 outline-none focus:bg-white/10 transition-colors"
          placeholder="Great post!"
        />
      </div>

      <!-- Zap button -->
      <button
        class="w-full py-2.5 rounded-lg text-xs font-medium bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors disabled:opacity-30"
        :disabled="!amount || amount < 1 || isZapping"
        @click="sendZap"
      >
        {{ isZapping ? 'Generating invoice...' : `Zap ${formatSats(amount)} sats` }}
      </button>

      <!-- Invoice / QR -->
      <div v-if="invoice" class="space-y-2">
        <p class="text-xs text-white/30 text-center">Scan or tap to pay</p>

        <!-- QR placeholder (canvas) -->
        <div class="flex justify-center">
          <canvas ref="qrCanvas" class="rounded-lg" width="200" height="200" />
        </div>

        <!-- Invoice string -->
        <div class="flex gap-1">
          <input
            :value="invoice"
            readonly
            class="flex-1 px-2 py-1.5 rounded text-base bg-white/5 text-white/40 font-mono truncate outline-none"
          />
          <button
            class="px-2 py-1.5 rounded text-xs bg-white/5 text-white/40 hover:text-white/60 transition-colors"
            @click="copyInvoice"
          >
            {{ copied ? 'Copied' : 'Copy' }}
          </button>
        </div>

        <!-- Open in wallet -->
        <a
          :href="'lightning:' + invoice"
          class="block w-full py-2 rounded-lg text-xs text-center bg-accent/15 text-accent/80 hover:bg-accent/25 transition-colors"
        >
          Open in wallet
        </a>
      </div>

      <!-- Error -->
      <p v-if="error" class="text-xs text-red-400/60 text-center">{{ error }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'

const props = defineProps<{
  isOpen: boolean
  targetName: string
  lightningAddress?: string
}>()

const emit = defineEmits<{ close: [] }>()

const dialogRef = ref<HTMLElement | null>(null)
const closeButtonRef = ref<HTMLElement | null>(null)

const amountPresets = [21, 100, 500, 1000, 5000, 10000]
const amount = ref(21)
const message = ref('')
const invoice = ref('')
const isZapping = ref(false)
const error = ref('')
const copied = ref(false)
const qrCanvas = ref<HTMLCanvasElement | null>(null)

function formatSats(sats: number): string {
  if (sats >= 1000) return `${(sats / 1000).toFixed(sats % 1000 === 0 ? 0 : 1)}k`
  return String(sats)
}

function close() {
  emit('close')
  invoice.value = ''
  error.value = ''
  message.value = ''
}

async function sendZap() {
  if (!props.lightningAddress || !amount.value) return

  isZapping.value = true
  error.value = ''
  invoice.value = ''

  try {
    // Resolve LNURL from Lightning address
    const [name, domain] = props.lightningAddress.split('@')
    if (!name || !domain) throw new Error('Invalid Lightning address')

    const lnurlRes = await fetch(`https://${domain}/.well-known/lnurlp/${name}`)
    if (!lnurlRes.ok) throw new Error('Failed to fetch LNURL')

    const lnurlData = await lnurlRes.json()
    if (lnurlData.status === 'ERROR') throw new Error(lnurlData.reason || 'LNURL error')

    const msats = amount.value * 1000
    if (msats < (lnurlData.minSendable ?? 0)) throw new Error(`Minimum: ${Math.ceil((lnurlData.minSendable ?? 0) / 1000)} sats`)
    if (msats > (lnurlData.maxSendable ?? Infinity)) throw new Error(`Maximum: ${Math.floor((lnurlData.maxSendable ?? 0) / 1000)} sats`)

    // Request invoice
    let callbackUrl = lnurlData.callback
    const sep = callbackUrl.includes('?') ? '&' : '?'
    callbackUrl += `${sep}amount=${msats}`
    if (message.value) callbackUrl += `&comment=${encodeURIComponent(message.value)}`

    const invoiceRes = await fetch(callbackUrl)
    if (!invoiceRes.ok) throw new Error('Failed to get invoice')

    const invoiceData = await invoiceRes.json()
    if (invoiceData.status === 'ERROR') throw new Error(invoiceData.reason || 'Invoice error')

    invoice.value = invoiceData.pr

    // Draw QR
    await nextTick()
    drawQR(invoiceData.pr)
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Zap failed'
  } finally {
    isZapping.value = false
  }
}

function drawQR(data: string) {
  const canvas = qrCanvas.value
  if (!canvas) return

  // Simple QR placeholder — draw the invoice text in a styled box
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  ctx.fillStyle = '#1a1a1a'
  ctx.fillRect(0, 0, 200, 200)
  ctx.fillStyle = '#F7931A'
  ctx.font = '10px monospace'
  ctx.textAlign = 'center'

  // Wrap text
  const lines: string[] = []
  for (let i = 0; i < data.length; i += 30) {
    lines.push(data.slice(i, i + 30))
  }
  const startY = Math.max(10, 100 - (lines.length * 6))
  lines.forEach((line, i) => {
    ctx.fillText(line, 100, startY + i * 12)
  })
}

function copyInvoice() {
  navigator.clipboard.writeText(invoice.value)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}

function trapFocus(e: KeyboardEvent) {
  const dialog = dialogRef.value
  if (!dialog) return
  const focusable = dialog.querySelectorAll<HTMLElement>(
    'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
  )
  if (focusable.length === 0) return
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault()
    last.focus()
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault()
    first.focus()
  }
}

watch(() => props.isOpen, async (open) => {
  if (open) {
    await nextTick()
    closeButtonRef.value?.focus()
  } else {
    invoice.value = ''
    error.value = ''
  }
})
</script>
