<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
        @click.self="close"
      >
        <div class="absolute inset-0 bg-black/60 backdrop-blur-md"></div>
        <div
          ref="modalRef"
          class="glass-card p-6 w-full max-w-md relative z-10 overflow-hidden"
          role="dialog"
          aria-modal="true"
          @click.stop
        >
          <!-- Header -->
          <div class="flex items-center justify-between gap-4 mb-4">
            <div class="flex items-center gap-2 min-w-0">
              <button
                v-if="pane !== 'scan'"
                @click="goBack"
                class="p-2 -ml-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors shrink-0"
                aria-label="Back"
              >
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
                </svg>
              </button>
              <h3 class="text-xl font-semibold text-white truncate">{{ paneTitle }}</h3>
            </div>
            <button
              @click="close"
              class="p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors shrink-0"
              aria-label="Close"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>

          <Transition :name="direction === 'forward' ? 'pane-forward' : 'pane-back'" mode="out-in">
            <!-- ============ SCAN PANE ============ -->
            <div v-if="pane === 'scan'" key="scan">
              <!-- Chooser interstitial — every open: the user picks live camera
                   or a photo upload; neither starts until chosen. -->
              <div v-if="scanChoice === 'unset'" class="w-full rounded-xl bg-black/30 border border-white/10 mb-4 p-6 flex flex-col items-center gap-3">
                <svg class="w-10 h-10 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 8V6a2 2 0 012-2h2M3 16v2a2 2 0 002 2h2m10-16h2a2 2 0 012 2v2m-4 12h2a2 2 0 002-2v-2M7 12h10" />
                </svg>
                <p class="text-sm text-white/60 text-center">How do you want to read the QR?</p>
                <!-- hasNativeQr: on the companion (plain http, no getUserMedia)
                     the native bridge still provides a live camera. On plain-http
                     desktop the button stays VISIBLE — hiding it read as "the
                     scanner is gone" (operator, 2026-08-05); choosing it surfaces
                     the browser's HTTPS requirement with the fallbacks instead. -->
                <button @click="chooseCamera" class="glass-button w-full px-4 py-2.5 rounded-lg text-sm font-medium">
                  Scan with camera
                </button>
                <p v-if="liveCameraUnavailable && !hasNativeQr" class="text-[11px] text-white/40 text-center -mt-1">
                  Your browser only allows live camera on HTTPS pages — the photo and paste options below always work.
                </p>
                <button @click="photoInput?.click()" class="glass-button w-full px-4 py-2.5 rounded-lg text-sm font-medium">
                  Upload / take a photo of the QR
                </button>
              </div>

              <div v-else class="relative w-full aspect-square rounded-xl overflow-hidden bg-black/40 border border-white/10 mb-4">
                <!-- opacity (not v-if/v-show): the scanner needs the element, and a
                     source-less <video> flashes a native play glyph in Android WebViews -->
                <video ref="videoElement" class="w-full h-full object-cover transition-opacity duration-200" :class="isScanning ? 'opacity-100' : 'opacity-0'" autoplay muted playsinline></video>
                <!-- Viewfinder -->
                <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
                  <div class="scan-viewfinder"></div>
                </div>
                <!-- While auto-start is settling, show only a quiet spinner — the
                     Start/Take-photo buttons are the FALLBACK, not a splash screen -->
                <div v-if="!isScanning && autoStarting" class="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/50">
                  <svg class="w-8 h-8 text-white/40 animate-spin" fill="none" viewBox="0 0 24 24">
                    <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" />
                    <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v3a5 5 0 00-5 5H4z" />
                  </svg>
                </div>
                <div v-else-if="!isScanning" class="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/50">
                  <svg class="w-10 h-10 text-white/40" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z" />
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 13a3 3 0 11-6 0 3 3 0 016 0z" />
                  </svg>
                  <button v-if="!liveCameraUnavailable" @click="startScanning" class="glass-button px-4 py-2 rounded-lg text-sm font-medium">
                    {{ cameraError ? 'Retry camera' : 'Start camera' }}
                  </button>
                  <!-- Insecure-context fallback: live getUserMedia preview needs
                       HTTPS, but the native camera via a file input does not —
                       snap a photo of the QR and decode it locally. -->
                  <button @click="photoInput?.click()" class="glass-button px-4 py-2 rounded-lg text-sm font-medium">
                    Take photo of QR
                  </button>
                </div>
              </div>

              <!-- Single always-mounted picker input, shared by the chooser and
                   the in-camera fallback button -->
              <input
                ref="photoInput"
                type="file"
                accept="image/*"
                capture="environment"
                class="hidden"
                @change="onPhotoPicked"
              />

              <div class="mb-4 p-3 bg-white/5 rounded-lg min-h-[3rem] flex items-center justify-center">
                <p class="text-sm text-center" :class="scanStatusIsError ? 'text-red-400' : 'text-white/60'">
                  {{ scanStatus || 'Point the camera at a Lightning invoice, Bitcoin address, Cashu or Fedimint code' }}
                </p>
              </div>

              <!-- Paste fallback (also the path on camera-less nodes) -->
              <div class="flex gap-2">
                <input
                  v-model="pasteInput"
                  type="text"
                  placeholder="…or paste an invoice / address / token"
                  class="flex-1 input-glass font-mono text-xs"
                  @keydown.enter="submitPaste"
                />
                <button @click="submitPaste" :disabled="!pasteInput.trim()" class="glass-button px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-40">
                  Use
                </button>
              </div>
            </div>

            <!-- ============ AMOUNT / CONFIRM PANE ============ -->
            <div v-else-if="pane === 'amount'" key="amount">
              <!-- Rail + destination summary -->
              <div class="flex items-center gap-3 p-3 bg-white/5 rounded-lg mb-4">
                <span class="w-9 h-9 rounded-lg bg-white/10 flex items-center justify-center shrink-0" :class="railColor">
                  <span v-if="rail === 'onchain'" class="text-lg font-bold">&#x20bf;</span>
                  <svg v-else-if="rail === 'lightning'" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                  <!-- SVG, not 🥜: kiosk images ship no color-emoji font -->
                  <svg v-else-if="rail === 'cashu'" class="w-5 h-5" role="img" aria-label="Cashu" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 9V7a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2m2 4h10a2 2 0 002-2v-6a2 2 0 00-2-2H9a2 2 0 00-2 2v6a2 2 0 002 2zm7-5a2 2 0 11-4 0 2 2 0 014 0z" />
                  </svg>
                  <svg v-else class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a4 4 0 00-3-3.87M9 20H4v-2a4 4 0 013-3.87m6-.13a4 4 0 10-4-4 4 4 0 004 4zm6 0a4 4 0 10-3-6.65" />
                  </svg>
                </span>
                <div class="min-w-0">
                  <p class="text-sm font-medium text-white">{{ railLabel }}</p>
                  <p class="text-[11px] text-white/40 font-mono truncate">{{ destDisplay }}</p>
                </div>
              </div>

              <!-- Federation join is a confirm, not an amount entry -->
              <template v-if="action === 'fedimint-join'">
                <p class="text-sm text-white/70 mb-4">
                  This is a Fedimint federation invite. Join it to hold and send ecash backed by this federation.
                </p>
              </template>

              <template v-else>
                <!-- Amount -->
                <div class="mb-3">
                  <div class="flex items-center justify-between mb-1">
                    <label class="text-white/60 text-sm">Amount (sats)</label>
                    <span v-if="amountLocked" class="text-[11px] px-2 py-0.5 rounded-full bg-white/10 text-white/50">set by invoice</span>
                  </div>
                  <input
                    v-model.number="amount"
                    type="number"
                    min="1"
                    inputmode="numeric"
                    placeholder="0"
                    :disabled="amountLocked || sendMax"
                    class="w-full input-glass text-2xl font-semibold text-center disabled:opacity-70"
                  />
                </div>

                <!-- Presets + Max -->
                <div v-if="!amountLocked" class="grid grid-cols-5 gap-2 mb-4">
                  <button
                    v-for="preset in PRESETS"
                    :key="preset"
                    @click="applyPreset(preset)"
                    :disabled="balance !== null && preset > balance"
                    class="px-1 py-1.5 rounded-lg text-xs font-medium transition-colors disabled:opacity-30"
                    :class="!sendMax && amount === preset ? 'bg-white/20 text-white' : 'bg-white/5 text-white/60 hover:bg-white/10 hover:text-white/90'"
                  >{{ formatPreset(preset) }}</button>
                  <button
                    @click="toggleMax"
                    class="px-1 py-1.5 rounded-lg text-xs font-semibold transition-colors"
                    :class="sendMax ? 'bg-orange-500/25 text-orange-300 border border-orange-500/40' : 'bg-white/5 text-white/60 hover:bg-white/10 hover:text-white/90'"
                  >Max</button>
                </div>

                <!-- Live balance impact -->
                <div class="rounded-lg border border-white/10 divide-y divide-white/5 mb-4">
                  <div class="flex items-center justify-between px-3 py-2">
                    <span class="text-xs text-white/50">{{ railLabel }} balance</span>
                    <span class="text-sm font-medium" :class="railColor">
                      {{ balance === null ? '…' : balance.toLocaleString() + ' sats' }}
                    </span>
                  </div>
                  <div class="flex items-center justify-between px-3 py-2">
                    <span class="text-xs text-white/50">This send</span>
                    <span class="text-sm font-medium text-white/80">−{{ effectiveAmount.toLocaleString() }} sats</span>
                  </div>
                  <div class="flex items-center justify-between px-3 py-2">
                    <span class="text-xs text-white/50">Balance after</span>
                    <span class="text-sm font-semibold" :class="insufficient ? 'text-red-400' : 'text-white'">
                      {{ balance === null ? '…' : balanceAfter.toLocaleString() + ' sats' }}
                    </span>
                  </div>
                </div>
                <p v-if="insufficient" class="text-xs text-red-400 mb-3">Not enough {{ railLabel }} balance for this amount.</p>
                <p v-else-if="sendMax && rail === 'onchain'" class="text-xs text-white/50 mb-3">Sweeps the entire on-chain balance minus network fees.</p>
              </template>

              <div v-if="error" class="mb-3 alert-error text-sm">{{ error }}</div>

              <div class="flex gap-3">
                <button @click="goBack" class="flex-1 glass-button px-4 py-2 rounded-lg text-sm">Back</button>
                <button
                  @click="confirmSend"
                  :disabled="processing || (action !== 'fedimint-join' && (insufficient || (effectiveAmount <= 0 && !sendMax)))"
                  class="flex-1 glass-button glass-button-warning px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50"
                >
                  {{ processing ? 'Sending…' : confirmLabel }}
                </button>
              </div>
            </div>

            <!-- ============ SUCCESS PANE ============ -->
            <div v-else key="success" class="text-center py-2">
              <div class="scan-success-badge mx-auto mb-5">
                <ScreensaverRing size="badge" />
                <div class="scan-success-core">
                  <svg class="w-14 h-14 text-green-400" fill="none" stroke="currentColor" stroke-width="3" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
              </div>

              <div v-if="successAmount > 0" class="text-5xl font-black text-green-400 mb-1">
                {{ successAmount.toLocaleString() }}<span class="text-2xl font-bold text-green-400/70"> sats</span>
              </div>
              <div class="text-2xl font-bold tracking-widest text-white mb-2">{{ successVerb }}</div>
              <p class="text-sm text-white/50 mb-1">{{ successDetail }}</p>
              <p v-if="successRef" class="text-[11px] text-white/30 font-mono break-all px-4 mb-4">{{ successRef }}</p>

              <div v-if="balance !== null" class="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-white/5 text-xs text-white/60 mb-5">
                <span :class="railColor">{{ railLabel }}</span>
                <span>balance now {{ balance.toLocaleString() }} sats</span>
              </div>

              <button
                @click="close"
                class="w-full py-3 rounded-xl font-semibold text-base bg-white/10 hover:bg-white/20 text-white transition-colors"
              >
                Done
              </button>
            </div>
          </Transition>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, watch, onUnmounted, nextTick } from 'vue'
import QrScanner from 'qr-scanner'
import { rpcClient } from '@/api/rpc-client'
import { useAnimatedQRDecoder } from '@/composables/useAnimatedQRDecoder'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import { useBodyScrollLock } from '@/composables/useBodyScrollLock'
import ScreensaverRing from '@/components/ScreensaverRing.vue'

type Rail = 'onchain' | 'lightning' | 'cashu' | 'fedimint'
type Action = 'pay-invoice' | 'send-onchain' | 'redeem-token' | 'fedimint-join'
type Pane = 'scan' | 'amount' | 'success'

// JS bridge the Android companion injects: when present, live scanning is
// delegated to a native camera modal (styled like this one) — the WebView's
// getUserMedia preview lags, and over plain http it doesn't exist at all.
// Decodes come back through the window.__archyQr* callbacks; status lines
// (animated-QR progress, errors) mirror out to the native modal's strip.
interface ArchipelagoQrBridge {
  open(): void
  setStatus(message: string, isError: boolean): void
  close(): void
}
interface NativeWindow extends Window {
  ArchipelagoQr?: ArchipelagoQrBridge
  __archyQrResult?: (text: string) => void
  __archyQrCancelled?: () => void
}
const nativeWin = window as NativeWindow

const PRESETS = [21, 2100, 21000, 100000]

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ close: []; sent: [] }>()

const modalRef = ref<HTMLElement | null>(null)
useModalKeyboard(modalRef, computed(() => props.show), close)
useBodyScrollLock(computed(() => props.show))

// --- Pane state ---
const pane = ref<Pane>('scan')
const direction = ref<'forward' | 'back'>('forward')
const paneTitle = computed(() => {
  if (pane.value === 'scan') return 'Scan to send'
  if (pane.value === 'success') return 'Success'
  if (action.value === 'fedimint-join') return 'Join federation'
  if (action.value === 'redeem-token') return 'Redeem'
  return 'Send'
})

function goTo(p: Pane, dir: 'forward' | 'back' = 'forward') {
  direction.value = dir
  pane.value = p
}

function goBack() {
  if (pane.value === 'amount') {
    error.value = ''
    goTo('scan', 'back')
    // Only relight the camera when that's what the user chose; otherwise
    // they land back on the scan/upload chooser.
    nextTick(() => { if (scanChoice.value === 'camera' && !liveCameraUnavailable.value) startScanning() })
  } else if (pane.value === 'success') {
    close()
  }
}

// --- Scanner ---
const videoElement = ref<HTMLVideoElement | null>(null)
const isScanning = ref(false)
// True while a camera-start attempt is in flight (incl. the automatic one on
// open) — the fallback buttons stay hidden until it resolves, so they don't
// flash for a second on every open.
const autoStarting = ref(false)

// Camera-vs-photo chooser, shown on EVERY open (user request 2026-07-23):
// nothing starts until the user picks, and Back from a later pane returns to
// the live camera only if that's what they chose.
const scanChoice = ref<'unset' | 'camera'>('unset')

function chooseCamera() {
  if (startNativeScan()) return
  scanChoice.value = 'camera'
  void nextTick(() => startScanning())
}

// --- Native scanner (companion app) ---
const hasNativeQr = !!nativeWin.ArchipelagoQr
const nativeScanActive = ref(false)

function startNativeScan(): boolean {
  const bridge = nativeWin.ArchipelagoQr
  if (!bridge) return false
  nativeWin.__archyQrResult = (text: string) => handleScanned(text)
  nativeWin.__archyQrCancelled = () => { nativeScanActive.value = false }
  nativeScanActive.value = true
  bridge.open()
  return true
}
const scanStatus = ref('')
const scanStatusIsError = ref(false)
const cameraError = ref(false)
const pasteInput = ref('')
const qrScanner = ref<QrScanner | null>(null)
const animatedDecoder = useAnimatedQRDecoder()

async function startScanning() {
  cameraError.value = false
  scanStatusIsError.value = false
  autoStarting.value = true
  try {
    if (!videoElement.value) return
    if (!navigator.mediaDevices?.getUserMedia) {
      throw new Error(
        location.protocol !== 'https:' && !['localhost', '127.0.0.1'].includes(location.hostname)
          ? 'Camera needs HTTPS — paste the code below instead'
          : 'No camera available — paste the code below instead'
      )
    }
    qrScanner.value = new QrScanner(
      videoElement.value,
      (result) => handleScanned(result.data),
      {
        returnDetailedScanResult: true,
        highlightScanRegion: false,
        preferredCamera: 'environment',
        // With a native BarcodeDetector (Chrome/Android WebView) decoding is
        // hardware-cheap — scan at 10/s for a snappier lock-on. The 4/s cap
        // remains for the JS-worker fallback, where 10/s visibly lagged the
        // preview on phone WebViews.
        maxScansPerSecond: 'BarcodeDetector' in window ? 10 : 4,
      }
    )
    await qrScanner.value.start()
    isScanning.value = true
    scanStatus.value = ''
  } catch (err) {
    cameraError.value = true
    scanStatusIsError.value = true
    scanStatus.value = err instanceof Error ? err.message : 'Unable to access camera'
  } finally {
    autoStarting.value = false
  }
}

function stopScanning() {
  qrScanner.value?.stop()
  qrScanner.value?.destroy()
  qrScanner.value = null
  isScanning.value = false
  if (nativeScanActive.value) {
    nativeScanActive.value = false
    nativeWin.ArchipelagoQr?.close()
  }
}

// Mirror status lines onto the native modal while it's up — it covers the
// page, so this strip is the only feedback the user can see.
watch([scanStatus, scanStatusIsError], () => {
  if (nativeScanActive.value) {
    nativeWin.ArchipelagoQr?.setStatus(scanStatus.value, scanStatusIsError.value)
  }
})

function submitPaste() {
  const text = pasteInput.value.trim()
  if (!text) return
  handleScanned(text)
}

// --- Photo-capture fallback (works without a secure context) ---
// getUserMedia is unreachable over plain http (navigator.mediaDevices is
// undefined), but <input capture> opens the native camera in any browser,
// PWA or WebView; the shot is decoded locally by qr-scanner's scanImage.
const photoInput = ref<HTMLInputElement | null>(null)
const liveCameraUnavailable = computed(() => !navigator.mediaDevices?.getUserMedia)

async function onPhotoPicked(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  scanStatusIsError.value = false
  scanStatus.value = 'Reading photo…'
  try {
    handleScanned(await decodePhotoRobust(file))
  } catch {
    scanStatusIsError.value = true
    scanStatus.value = 'No QR code found in that photo — try again, closer and well-lit'
  } finally {
    input.value = ''
  }
}

/** Decode a QR photo with every engine we have. On the companion app the
 *  photo path IS the scan path (plain-http LAN = no secure context = no
 *  live camera), and Lightning invoices make DENSE codes that the wasm
 *  engine's single pass often misses (reported 2026-07-22: "camera not
 *  picking up the invoice"). Android WebView's native BarcodeDetector is
 *  far stronger on dense codes, so try it first; fall back to qr-scanner. */
async function decodePhotoRobust(file: File): Promise<string> {
  try {
    const Detector = (window as unknown as { BarcodeDetector?: new (opts: { formats: string[] }) => { detect(src: ImageBitmap): Promise<Array<{ rawValue: string }>> } }).BarcodeDetector
    if (Detector) {
      const bmp = await createImageBitmap(file)
      const codes = await new Detector({ formats: ['qr_code'] }).detect(bmp)
      const hit = codes.find(c => c.rawValue)
      if (hit) return hit.rawValue
    }
  } catch {
    // Native detector unavailable/failed — wasm engine below.
  }
  const result = await QrScanner.scanImage(file, { returnDetailedScanResult: true })
  return result.data
}

// --- Detection (ported from k484's scanner) ---
const rail = ref<Rail>('lightning')
const action = ref<Action>('pay-invoice')
const dest = ref('')
const amount = ref<number>(0)
const amountLocked = ref(false)
const sendMax = ref(false)

const railLabel = computed(() => ({
  onchain: 'On-chain',
  lightning: 'Lightning',
  cashu: 'Cashu',
  fedimint: 'Fedimint',
}[rail.value]))

const railColor = computed(() => ({
  onchain: 'text-orange-500',
  lightning: 'text-yellow-400',
  cashu: 'text-purple-400',
  fedimint: 'text-blue-400',
}[rail.value]))

const destDisplay = computed(() =>
  dest.value.length > 46 ? `${dest.value.slice(0, 26)}…${dest.value.slice(-14)}` : dest.value
)

const confirmLabel = computed(() => {
  if (action.value === 'fedimint-join') return 'Join'
  if (action.value === 'redeem-token') return 'Redeem'
  return 'Send'
})

/** Amount encoded in a BOLT11 invoice's human-readable part, in sats (null = zero-amount). */
function parseBolt11AmountSats(invoice: string): number | null {
  const m = /^ln(?:bcrt|bc|tb)(\d+)?([munp])?1/.exec(invoice.toLowerCase())
  if (!m || !m[1]) return null
  const value = Number(m[1])
  const mult = { m: 1e-3, u: 1e-6, n: 1e-9, p: 1e-12 }[m[2] as 'm' | 'u' | 'n' | 'p'] ?? 1
  return Math.round(value * mult * 1e8)
}

function isLightningInvoice(text: string): boolean {
  const t = text.toLowerCase()
  return t.startsWith('lnbc') || t.startsWith('lntb') || t.startsWith('lnbcrt')
}

function isOnchainAddress(text: string): boolean {
  return /^(bc1|tb1|bcrt1)[a-z0-9]{20,90}$/i.test(text) || /^[13][a-km-zA-HJ-NP-Z1-9]{25,40}$/.test(text)
}

function isCashuToken(text: string): boolean {
  return text.startsWith('cashuA') || text.startsWith('cashuB')
}

function isFedimintInvite(text: string): boolean {
  return text.startsWith('fed11')
}

function isFedimintToken(text: string): boolean {
  return /^AwE/.test(text)
}

function isAnimatedFrame(text: string): boolean {
  return text.startsWith('B$') || text.startsWith('AAAH') || text.startsWith('ZAAE') || text.startsWith('AAA')
}

function handleScanned(raw: string) {
  let text = raw.trim()
  scanStatusIsError.value = false

  // Animated multi-frame QR (large Fedimint tokens): keep collecting frames
  if (isAnimatedFrame(text)) {
    animatedDecoder.addFrame(text)
    if (animatedDecoder.isComplete.value && animatedDecoder.decodedData.value) {
      const token = animatedDecoder.decodedData.value
      animatedDecoder.reset()
      acceptDetected(token)
    } else {
      scanStatus.value = `Animated code… ${animatedDecoder.progressText()}`
    }
    return
  }

  acceptDetected(text)
}

function acceptDetected(raw: string) {
  let text = raw.trim()
  let bip21AmountSats: number | null = null

  // URI schemes
  if (text.toLowerCase().startsWith('lightning:')) text = text.slice(10)
  if (text.toLowerCase().startsWith('bitcoin:')) {
    try {
      const uri = new URL(text)
      const lnParam = uri.searchParams.get('lightning')
      const amt = uri.searchParams.get('amount')
      if (amt && !Number.isNaN(Number(amt))) bip21AmountSats = Math.round(Number(amt) * 1e8)
      // Prefer the unified lightning param when present, else the address
      text = lnParam || text.slice(8).split('?')[0] || ''
    } catch {
      text = text.slice(8).split('?')[0] || ''
    }
  }

  if (isLightningInvoice(text)) {
    rail.value = 'lightning'
    action.value = 'pay-invoice'
    dest.value = text
    const invoiceAmount = parseBolt11AmountSats(text)
    // Zero-amount invoice inside a unified BIP21 URI: prefill from amount=
    amount.value = invoiceAmount ?? bip21AmountSats ?? 0
    amountLocked.value = invoiceAmount !== null
  } else if (isCashuToken(text)) {
    rail.value = 'cashu'
    action.value = 'redeem-token'
    dest.value = text
    amount.value = 0
    amountLocked.value = true
  } else if (isFedimintInvite(text)) {
    rail.value = 'fedimint'
    action.value = 'fedimint-join'
    dest.value = text
    amount.value = 0
    amountLocked.value = true
  } else if (isFedimintToken(text)) {
    rail.value = 'fedimint'
    action.value = 'redeem-token'
    dest.value = text
    amount.value = 0
    amountLocked.value = true
  } else if (isOnchainAddress(text)) {
    rail.value = 'onchain'
    action.value = 'send-onchain'
    dest.value = text
    amount.value = bip21AmountSats ?? 0
    amountLocked.value = false
  } else if (text.toLowerCase().startsWith('lno1')) {
    // BOLT12 offer — LND (this node's Lightning backend) can't pay offers yet
    scanStatusIsError.value = true
    scanStatus.value = 'BOLT12 offers aren\'t supported yet — ask the recipient for a BOLT11 invoice'
    return
  } else if (text.toLowerCase().startsWith('lnurl1') || /^[\w.+-]+@[\w-]+(\.[\w-]+)+$/.test(text)) {
    // LNURL-pay / lightning address — needs an HTTP callback flow we don't do yet
    scanStatusIsError.value = true
    scanStatus.value = 'LNURL and lightning addresses aren\'t supported yet — ask for a BOLT11 invoice'
    return
  } else {
    scanStatusIsError.value = true
    scanStatus.value = 'Not a recognised invoice, address or token'
    return
  }

  sendMax.value = false
  error.value = ''
  stopScanning()
  loadBalance()

  // Tokens carry their own value — redeem straight away, success screen follows
  if (action.value === 'redeem-token') {
    goTo('amount')
    confirmSend()
  } else {
    goTo('amount')
  }
}

// --- Balance for the active rail ---
const balance = ref<number | null>(null)

async function loadBalance() {
  balance.value = null
  try {
    if (rail.value === 'cashu') {
      const res = await rpcClient.call<{ balance_sats: number }>({ method: 'wallet.ecash-balance' })
      balance.value = res.balance_sats ?? 0
    } else if (rail.value === 'fedimint') {
      const res = await rpcClient.call<{ balance_sats: number }>({ method: 'wallet.fedimint-balance' })
      balance.value = res.balance_sats ?? 0
    } else {
      const res = await rpcClient.call<{ balance_sats: number; channel_balance_sats: number }>({ method: 'lnd.getinfo' })
      balance.value = rail.value === 'onchain' ? (res.balance_sats ?? 0) : (res.channel_balance_sats ?? 0)
    }
  } catch {
    balance.value = null
  }
}

const effectiveAmount = computed(() => {
  if (sendMax.value) return balance.value ?? 0
  return amount.value > 0 ? Math.floor(amount.value) : 0
})
const balanceAfter = computed(() => (balance.value ?? 0) - effectiveAmount.value)
const insufficient = computed(() =>
  action.value !== 'fedimint-join' && action.value !== 'redeem-token' &&
  balance.value !== null && effectiveAmount.value > balance.value
)

function applyPreset(preset: number) {
  sendMax.value = false
  amount.value = preset
}

function toggleMax() {
  sendMax.value = !sendMax.value
  if (sendMax.value && balance.value !== null && rail.value !== 'onchain') {
    amount.value = balance.value
  }
}

function formatPreset(preset: number): string {
  return preset >= 1000 ? `${(preset / 1000).toLocaleString()}k` : String(preset)
}

// --- Send / redeem / join ---
const processing = ref(false)
const error = ref('')
const successAmount = ref(0)
const successVerb = ref('SENT')
const successDetail = ref('')
const successRef = ref('')

async function confirmSend() {
  if (processing.value) return
  processing.value = true
  error.value = ''
  try {
    if (action.value === 'pay-invoice') {
      const params: { payment_request: string; amount_sats?: number } = { payment_request: dest.value }
      if (!amountLocked.value && effectiveAmount.value > 0) params.amount_sats = effectiveAmount.value
      // Waits out slow multi-hop routing and only reports failure when LND
      // itself declares the payment failed — never on a timeout. Pending
      // (~8s) jumps straight to the success pane in SETTLING mode instead of
      // spinning through the whole poll; the poll upgrades it to PAID.
      const res = await rpcClient.payLightningInvoice(params, (hash) => {
        successAmount.value = effectiveAmount.value
        successVerb.value = 'SETTLING'
        successDetail.value = 'Payment is on its way — this updates the moment it settles. Safe to close.'
        successRef.value = hash
        processing.value = false
        goTo('success')
        emit('sent')
      })
      if (res.status === 'failed') throw new Error(res.failure_reason || 'Payment failed')
      successAmount.value = res.amount_sats || effectiveAmount.value
      successVerb.value = res.status === 'pending' ? 'SENDING' : 'PAID'
      successDetail.value = res.status === 'pending'
        ? 'Payment in flight — it will appear in your transactions once it settles'
        : 'Lightning invoice paid'
      successRef.value = res.payment_hash
    } else if (action.value === 'send-onchain') {
      const res = await rpcClient.call<{ txid: string }>({
        method: 'lnd.sendcoins',
        params: sendMax.value
          ? { addr: dest.value, send_all: true }
          : { addr: dest.value, amount: effectiveAmount.value },
        timeout: 60000,
      })
      successAmount.value = sendMax.value ? (balance.value ?? 0) : effectiveAmount.value
      successVerb.value = 'SENT'
      successDetail.value = 'On-chain transaction broadcast'
      successRef.value = res.txid
    } else if (action.value === 'redeem-token') {
      const res = await rpcClient.call<{ received_sats: number; kind: string }>({
        method: 'wallet.ecash-receive',
        params: { token: dest.value },
        timeout: 60000,
      })
      rail.value = res.kind === 'fedimint' ? 'fedimint' : 'cashu'
      successAmount.value = res.received_sats || 0
      successVerb.value = 'RECEIVED'
      successDetail.value = `${res.kind === 'fedimint' ? 'Fedimint' : 'Cashu'} token redeemed`
      successRef.value = ''
    } else {
      await rpcClient.call({
        method: 'wallet.fedimint-join',
        params: { invite_code: dest.value },
        timeout: 60000,
      })
      successAmount.value = 0
      successVerb.value = 'JOINED'
      successDetail.value = 'Federation joined — you can now hold its ecash'
      successRef.value = ''
    }
    await loadBalance()
    goTo('success')
    emit('sent')
  } catch (err) {
    error.value = err instanceof Error ? err.message : 'Send failed'
    // Token redeems jump straight from scan; surface the failure on the amount pane
    if (pane.value !== 'amount') goTo('amount')
  } finally {
    processing.value = false
  }
}

// --- Lifecycle ---
function resetAll() {
  stopScanning()
  animatedDecoder.reset()
  pane.value = 'scan'
  direction.value = 'forward'
  scanStatus.value = ''
  scanStatusIsError.value = false
  cameraError.value = false
  pasteInput.value = ''
  dest.value = ''
  amount.value = 0
  amountLocked.value = false
  sendMax.value = false
  balance.value = null
  processing.value = false
  error.value = ''
}

function close() {
  resetAll()
  emit('close')
}

watch(() => props.show, (open) => {
  if (open) {
    resetAll()
    // No auto-start: the chooser interstitial owns the first move every time.
    scanChoice.value = 'unset'
  } else {
    stopScanning()
  }
})

onUnmounted(stopScanning)
</script>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}
.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}
.modal-enter-active .glass-card,
.modal-leave-active .glass-card {
  transition: transform 0.2s ease;
}
.modal-enter-from .glass-card,
.modal-leave-to .glass-card {
  transform: scale(0.95);
}

/* Horizontal pane slide: forward = new pane arrives from the right */
.pane-forward-enter-active,
.pane-forward-leave-active,
.pane-back-enter-active,
.pane-back-leave-active {
  transition: transform 0.22s ease, opacity 0.22s ease;
}
.pane-forward-enter-from {
  transform: translateX(40px);
  opacity: 0;
}
.pane-forward-leave-to {
  transform: translateX(-40px);
  opacity: 0;
}
.pane-back-enter-from {
  transform: translateX(-40px);
  opacity: 0;
}
.pane-back-leave-to {
  transform: translateX(40px);
  opacity: 0;
}

.scan-viewfinder {
  width: 62%;
  height: 62%;
  border-radius: 1rem;
  border: 2px solid rgba(249, 115, 22, 0.85);
  box-shadow: 0 0 0 9999px rgba(0, 0, 0, 0.35);
}

/* Paid tick (FED-06) — same composition as SendBitcoinModal: the branded
   ScreensaverRing badge with the emerald checkmark core centred over it. */
.scan-success-badge {
  position: relative;
  width: 160px;
  height: 160px;
  display: flex;
  align-items: center;
  justify-content: center;
}
@media (min-width: 768px) {
  .scan-success-badge {
    width: 192px;
    height: 192px;
  }
}
.scan-success-core {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  width: 6rem;
  height: 6rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 9999px;
  background: rgba(16, 185, 129, 0.12);
  box-shadow: 0 0 40px rgba(16, 185, 129, 0.25);
}
</style>
