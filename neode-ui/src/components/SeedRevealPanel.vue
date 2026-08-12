<template>
  <div>
    <div class="flex gap-1 mb-3 p-1 bg-white/5 rounded-lg">
      <button
        v-for="tab in ([{ key: 'words', label: 'Words' }, { key: 'qr', label: 'QR code' }] as const)"
        :key="tab.key"
        type="button"
        @click="seedTab = tab.key"
        class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
        :class="seedTab === tab.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
      >{{ tab.label }}</button>
    </div>

    <template v-if="seedTab === 'words'">
      <p class="text-sm text-white/60 mb-3">Write these down and store them offline. Tap to {{ hidden ? 'reveal' : 'hide' }}.</p>
      <div class="relative">
        <div
          class="grid grid-cols-2 sm:grid-cols-3 gap-2 p-3 bg-white/5 rounded-lg transition-all select-text"
          :class="hidden ? 'blur-md' : ''"
          @click="hidden = !hidden"
        >
          <div v-for="(w, i) in words" :key="i" class="flex items-center gap-1.5 text-sm">
            <span class="text-white/30 text-xs w-5 text-right">{{ i + 1 }}.</span>
            <span class="text-white font-mono">{{ w }}</span>
          </div>
        </div>
        <button v-if="hidden" type="button" class="absolute inset-0 flex items-center justify-center text-xs text-white/70 font-medium" @click="hidden = false">Tap to reveal</button>
      </div>
    </template>

    <template v-else>
      <p class="text-sm text-white/60 mb-3">
        {{ aezeed ? 'Scan to copy the words into another device.' : 'Scan into a wallet that imports seeds by QR.' }}
        Tap to {{ hidden ? 'reveal' : 'hide' }}.
      </p>
      <div class="relative">
        <div
          class="flex justify-center p-3 bg-white/5 rounded-lg transition-all"
          :class="hidden ? 'blur-md' : ''"
          @click="hidden = !hidden"
        >
          <canvas ref="qrCanvas" class="rounded-lg bg-white p-2"></canvas>
        </div>
        <button v-if="hidden" type="button" class="absolute inset-0 flex items-center justify-center text-xs text-white/70 font-medium" @click="hidden = false">Tap to reveal</button>
      </div>
      <div v-if="!aezeed && seedQrAvailable" class="flex justify-center mt-2">
        <div class="flex p-0.5 bg-white/5 rounded-md">
          <button
            v-for="f in ([{ key: 'seedqr', label: 'SeedQR' }, { key: 'text', label: 'Plain text' }] as const)"
            :key="f.key"
            type="button"
            @click="qrFormat = f.key"
            class="px-2.5 py-1 rounded text-[11px] font-medium transition-colors"
            :class="qrFormat === f.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
          >{{ f.label }}</button>
        </div>
      </div>
      <p class="text-xs text-white/40 mt-2">
        <template v-if="aezeed">
          The code contains your seed words as plain text — treat it exactly like the words
          themselves. Note: this is an LND <span class="font-mono">aezeed</span>, not a BIP39
          phrase — it restores into LND-based wallets (Zeus, Blixt, another Archipelago node),
          not into hardware wallets like Passport.
        </template>
        <template v-else-if="qrFormat === 'seedqr'">
          SeedQR — scans into Passport, SeedSigner, Keystone and other wallets that import
          seeds by QR. Treat this code exactly like the words themselves.
        </template>
        <template v-else>
          Plain text words — for wallets that read the phrase as text. Treat this code exactly
          like the words themselves.
        </template>
      </p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'

// Shared seed reveal body: Words / QR code tabs behind a tap-to-reveal blur.
// Words are always the first view. For BIP39 seeds the QR defaults to the
// SeedQR standard (4-digit wordlist indices — what Passport Prime, SeedSigner,
// Keystone etc. import), with a plain-text option. `aezeed` seeds (LND) are
// NOT BIP39 and no hardware wallet can import them, so they only ever get the
// plain-text QR plus an explanation — SeedQR-encoding one would be dishonest.
const props = defineProps<{ words: string[]; aezeed?: boolean }>()

const seedTab = ref<'words' | 'qr'>('words')
const qrFormat = ref<'seedqr' | 'text'>(props.aezeed ? 'text' : 'seedqr')
const seedQrAvailable = ref(!props.aezeed)
const hidden = ref(true)
const qrCanvas = ref<HTMLCanvasElement | null>(null)

async function renderQr() {
  await nextTick()
  if (!qrCanvas.value || props.words.length === 0) return
  try {
    let payload = props.words.join(' ')
    if (!props.aezeed && qrFormat.value === 'seedqr') {
      const { toSeedQrDigits } = await import('@/utils/seedqr')
      const digits = await toSeedQrDigits(props.words)
      if (digits) {
        payload = digits
      } else {
        // Not a BIP39 phrase after all — only plain text is honest.
        seedQrAvailable.value = false
        qrFormat.value = 'text'
        return // the qrFormat watcher re-renders as text
      }
    }
    const QRCode = await import('qrcode')
    await QRCode.toCanvas(qrCanvas.value, payload, { width: 260, margin: 1 })
  } catch { /* QR is a convenience — the words remain authoritative */ }
}
watch(seedTab, (t) => { if (t === 'qr') void renderQr() })
watch(qrFormat, () => { if (seedTab.value === 'qr') void renderQr() })
watch(() => props.words, () => {
  seedTab.value = 'words' // fresh reveal always shows words first
  hidden.value = true
  seedQrAvailable.value = !props.aezeed
  qrFormat.value = props.aezeed ? 'text' : 'seedqr'
})
</script>
