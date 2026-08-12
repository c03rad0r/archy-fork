<template>
  <div class="h-[100dvh] flex items-center justify-center p-3 sm:p-4 md:p-6">
    <div class="max-w-[800px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Header -->
      <div class="text-center flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6 pb-2 sm:pb-3">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-1.5 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Your Recovery Seed
        </h1>
        <p class="text-xs sm:text-sm md:text-base text-white/75 leading-relaxed max-w-[600px] mx-auto">
          Write down these 24 words in order. They are the only way to recover your node.
        </p>
      </div>

      <!-- Scrollable Content -->
      <div ref="scrollContainer" class="flex-1 overflow-y-auto overflow-x-hidden px-6 sm:px-8 min-h-0">
        <div ref="contentWrapper" class="flex flex-col items-center gap-3 sm:gap-4 py-3">
          <!-- Loading State -->
          <div v-if="loading" class="text-center py-8">
            <div class="flex justify-center mb-4">
              <div class="w-16 h-16 rounded-full bg-white/10 flex items-center justify-center onb-lock-spin">
                <svg class="w-8 h-8 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                </svg>
              </div>
            </div>
            <div v-if="waitingForServer" class="flex items-center justify-center gap-3 mb-2">
              <p class="text-lg text-white/80">Server starting up</p>
              <span class="text-sm text-white/40 font-mono tabular-nums">{{ elapsedDisplay }}</span>
            </div>
            <p v-if="waitingForServer" class="text-sm text-white/50">This usually takes 1-3 minutes after first boot</p>
            <p v-else class="text-lg text-white/80">Generating your seed phrase...</p>
          </div>

          <!-- Error (genuine failure — server-starting hiccups retry silently) -->
          <div v-if="errorMessage" class="text-center">
            <p class="text-red-400 text-sm mb-3">{{ errorMessage }}</p>
            <button
              @click="generateSeed"
              class="path-action-button path-action-button--continue mx-auto"
            >
              Try again
            </button>
          </div>

          <!-- Word Grid -->
          <div v-if="words.length > 0" class="w-full max-w-[600px]">
            <!-- Words / QR tabs — words first; QR for wallets that import by scan -->
            <div class="flex gap-1 mb-2 p-1 bg-white/5 rounded-lg">
              <button
                v-for="tab in ([{ key: 'words', label: 'Words' }, { key: 'qr', label: 'QR code' }] as const)"
                :key="tab.key"
                type="button"
                @click="seedTab = tab.key"
                class="flex-1 px-2 py-1.5 rounded text-xs font-medium transition-colors"
                :class="seedTab === tab.key ? 'bg-white/15 text-white' : 'text-white/50 hover:text-white/80'"
              >{{ tab.label }}</button>
            </div>

            <div v-if="seedTab === 'words'" class="grid grid-cols-2 sm:grid-cols-4 gap-1 sm:gap-1.5">
              <div
                v-for="(word, i) in words"
                :key="i"
                class="bg-black/60 rounded-lg px-2.5 py-1 sm:py-1.5 border border-white/10"
              >
                <span class="text-white/40 text-sm font-mono mr-1">{{ i + 1 }}.</span>
                <span class="text-white/95 text-[1.05rem] font-mono">{{ word }}</span>
              </div>
            </div>

            <div v-else class="flex flex-col items-center gap-2 py-2">
              <canvas ref="seedQrCanvas" class="rounded-lg bg-white p-2"></canvas>
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
              <p class="text-xs text-white/50 text-center max-w-[420px]">
                {{ qrFormat === 'seedqr'
                  ? 'SeedQR — scans into Passport, SeedSigner, Keystone and other wallets that import seeds by QR.'
                  : 'Plain text words — for wallets that read the phrase as text.' }}
                Treat this code exactly like the words themselves.
              </p>
            </div>

            <!-- Warning -->
            <div class="mt-3 bg-orange-500/10 border border-orange-500/20 rounded-lg px-3 py-2.5">
              <p class="text-xs sm:text-sm text-orange-300/90">
                Never share these words. Anyone with them controls your node, identities, and Bitcoin wallet.
              </p>
            </div>

            <!-- Confirmation Checkbox -->
            <label ref="confirmLabel" class="flex items-center justify-center gap-3 mt-3 cursor-pointer select-none">
              <input
                v-model="confirmed"
                type="checkbox"
                class="w-5 h-5 rounded border-white/20 bg-black/40 accent-orange-400"
              />
              <span class="text-xs sm:text-sm text-white/80">I have written down these 24 words in a safe place</span>
            </label>
          </div>
        </div>
        <Transition name="onb-cue-fade">
          <div v-if="showScrollCue" class="sticky bottom-0 inset-x-0 h-16 flex items-end justify-center pointer-events-none">
            <div class="absolute inset-0 bg-gradient-to-t from-black/65 to-transparent"></div>
            <button
              type="button"
              class="relative z-10 mb-2 pointer-events-auto inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full bg-black/60 backdrop-blur-md border border-white/10 text-white/75 text-xs"
              aria-label="Scroll down to the confirmation checkbox"
              @click="revealConfirm"
            >
              <span>One more step below</span>
              <svg class="onb-cue-chevron w-4 h-4 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7" />
              </svg>
            </button>
          </div>
        </Transition>
      </div>

      <!-- Fixed Footer -->
      <div v-if="words.length > 0" class="flex-shrink-0 flex justify-center px-3 sm:px-4 pt-3 pb-4 sm:pb-6">
        <button
          ref="continueButton"
          @click="proceed"
          :disabled="!confirmed"
          class="path-action-button path-action-button--continue disabled:opacity-50"
        >
          Continue
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const continueButton = ref<HTMLButtonElement | null>(null)
const words = ref<string[]>([])

// Bottom scroll cue (UIFIX-03) — on short viewports the confirmation
// checkbox sits below the fold inside the scrolling region while the
// Continue button stays pinned, disabled, in the fixed footer below. The
// cue is a pure wayfinding affordance: it only appears when the scroll
// region actually has more content below the fold than fits, and it never
// touches `confirmed` or the Continue button itself.
const scrollContainer = ref<HTMLElement | null>(null)
const contentWrapper = ref<HTMLElement | null>(null)
const confirmLabel = ref<HTMLElement | null>(null)
const showScrollCue = ref(false)
let cueResizeObserver: ResizeObserver | null = null

function updateScrollCue() {
  const container = scrollContainer.value
  const label = confirmLabel.value
  if (!container || !label || loading.value || words.value.length === 0 || confirmed.value) {
    showScrollCue.value = false
    return
  }
  const hasOverflow = container.scrollHeight > container.clientHeight
  if (!hasOverflow) {
    showScrollCue.value = false
    return
  }
  // Viewport-relative rects rather than offsetTop/offsetHeight — offsetTop is
  // relative to the nearest *positioned* ancestor (here, the outer card,
  // which carries `relative` for its own z-index stacking), not necessarily
  // this scroll container, so it cannot be trusted to measure "below the
  // scroll region's visible bottom".
  const labelBottom = label.getBoundingClientRect().bottom
  const containerBottom = container.getBoundingClientRect().bottom
  showScrollCue.value = labelBottom > containerBottom
}

// Wayfinding only — this must never set `confirmed`, never focus/enable the
// Continue button, and never call proceed().
function revealConfirm() {
  confirmLabel.value?.scrollIntoView({ behavior: 'smooth', block: 'center' })
}

// Words / QR code view of the seed — words are the default first view.
// The QR tab defaults to SeedQR (BIP39 word-index digit stream), the format
// hardware wallets like Passport Prime / SeedSigner actually import; plain
// text stays available for wallets that read the phrase as text.
const seedTab = ref<'words' | 'qr'>('words')
const qrFormat = ref<'seedqr' | 'text'>('seedqr')
const seedQrCanvas = ref<HTMLCanvasElement | null>(null)

async function renderSeedQr() {
  await nextTick()
  if (!seedQrCanvas.value || words.value.length === 0) return
  try {
    let payload = words.value.join(' ')
    if (qrFormat.value === 'seedqr') {
      const { toSeedQrDigits } = await import('@/utils/seedqr')
      const digits = await toSeedQrDigits(words.value)
      if (digits) payload = digits
      else qrFormat.value = 'text' // non-BIP39 word — only text is honest
    }
    const QRCode = await import('qrcode')
    await QRCode.toCanvas(seedQrCanvas.value, payload, { width: 240, margin: 1 })
  } catch { /* QR is a convenience — the words remain authoritative */ }
}
watch(seedTab, (tab) => { if (tab === 'qr') void renderSeedQr() })
watch(qrFormat, () => { if (seedTab.value === 'qr') void renderSeedQr() })
const confirmed = ref(false)
const loading = ref(false)
const waitingForServer = ref(false)
const errorMessage = ref('')
const elapsedDisplay = ref('0:00')
let retryTimer: ReturnType<typeof setTimeout> | null = null
let elapsedTimer: ReturnType<typeof setInterval> | null = null
let startTime = 0

function startElapsedTimer() {
  startTime = Date.now()
  elapsedTimer = setInterval(() => {
    const secs = Math.floor((Date.now() - startTime) / 1000)
    const m = Math.floor(secs / 60)
    const s = secs % 60
    elapsedDisplay.value = `${m}:${s.toString().padStart(2, '0')}`
  }, 1000)
}

function stopTimers() {
  if (retryTimer) { clearTimeout(retryTimer); retryTimer = null }
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null }
}

// Transient errors mean the backend is still booting (slow first boot) — we
// retry these silently. Anything else is a genuine failure the user should see.
function isServerStartingError(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : String(err)
  return /502|503|504|timeout|fetch|network|Failed to fetch|Request failed/i.test(msg)
}

async function generateSeed() {
  loading.value = true
  errorMessage.value = ''

  try {
    // seed.generate is idempotent server-side, so retries are safe. Use a
    // longer timeout than the default 15s — key derivation + disk writes can be
    // slow on first boot, and we don't want a spurious abort to look like a
    // failure.
    const res = await rpcClient.call<{ words: string[] }>({ method: 'seed.generate', timeout: 30000 })
    stopTimers()
    words.value = res.words
    loading.value = false
    waitingForServer.value = false
  } catch (err) {
    if (isServerStartingError(err)) {
      // Backend not ready yet — keep waiting, retry silently. `loading` stays
      // true through the whole retry loop: dropping it here unmounts the lock
      // icon and status text for the 4s between attempts, which reads as the
      // screen flashing in and out (reported on a live install test).
      if (!waitingForServer.value) {
        waitingForServer.value = true
        startElapsedTimer()
      }
      retryTimer = setTimeout(generateSeed, 4000)
    } else {
      // Genuine failure — stop the silent loop and surface it with a manual retry.
      stopTimers()
      loading.value = false
      waitingForServer.value = false
      const raw = err instanceof Error ? err.message : 'Failed to generate seed'
      // The backend's provisioned-guard refusal is precise but written for
      // developers ("authenticated system.factory-reset"). Operators hit it
      // when a node that already has an identity lands on this screen —
      // translate it into what they can actually do about it.
      errorMessage.value = raw.startsWith('Not supported: this node is already provisioned')
        ? 'This node already has an identity, so a new seed cannot be created. Sign in normally — or to start this node over, run a factory reset from Settings first.'
        : raw
    }
  }
}

watch(confirmed, (val) => {
  if (val) {
    nextTick(() => {
      setTimeout(() => continueButton.value?.focus({ preventScroll: true }), 100)
    })
  }
  updateScrollCue()
})

// Words arrive asynchronously (RPC or restored from sessionStorage) and the
// word grid changes height when the words/QR tabs switch — both can flip the
// scroll region from non-overflowing to overflowing, so re-measure whenever
// either happens.
watch(words, () => { nextTick(updateScrollCue) })
watch(loading, () => { nextTick(updateScrollCue) })

onMounted(() => {
  // Restore previously generated seed if navigating back (don't regenerate)
  const saved = sessionStorage.getItem('_seed_words')
  let restored = false
  const setup = () => {
    scrollContainer.value?.addEventListener('scroll', updateScrollCue)
    window.addEventListener('resize', updateScrollCue)
    if (contentWrapper.value && typeof ResizeObserver !== 'undefined') {
      cueResizeObserver = new ResizeObserver(() => updateScrollCue())
      cueResizeObserver.observe(contentWrapper.value)
    }
    nextTick(updateScrollCue)
  }
  if (saved) {
    try {
      const parsed = JSON.parse(saved)
      if (Array.isArray(parsed) && parsed.length === 24) {
        words.value = parsed
        restored = true
      }
    } catch { /* regenerate */ }
  }
  setup()
  if (!restored) generateSeed()
})
onUnmounted(() => {
  stopTimers()
  scrollContainer.value?.removeEventListener('scroll', updateScrollCue)
  window.removeEventListener('resize', updateScrollCue)
  cueResizeObserver?.disconnect()
  cueResizeObserver = null
})

function proceed() {
  playNavSound('action')
  sessionStorage.setItem('_seed_words', JSON.stringify(words.value))
  router.push('/onboarding/seed-verify').catch(() => {})
}
</script>

<style scoped>
.onb-lock-spin {
  animation: onb-lock-pulse 1.2s ease-in-out infinite;
}
@keyframes onb-lock-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(1.08); opacity: 0.7; }
}

/* Bottom scroll cue (UIFIX-03) — chevron bob + fade transition. */
.onb-cue-chevron {
  animation: onb-cue-bob 2s ease-in-out infinite;
}
@keyframes onb-cue-bob {
  0%, 100% { transform: translateY(0); }
  50% { transform: translateY(3px); }
}
.onb-cue-fade-enter-active,
.onb-cue-fade-leave-active {
  transition: opacity 0.2s ease;
}
.onb-cue-fade-enter-from,
.onb-cue-fade-leave-to {
  opacity: 0;
}
@media (prefers-reduced-motion: reduce) {
  .onb-cue-chevron { animation: none; }
}
</style>
