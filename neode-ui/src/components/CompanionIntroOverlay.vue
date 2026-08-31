<template>
  <Teleport to="body">
    <Transition name="overlay-fade">
      <div
        v-if="visible"
        class="fixed inset-0 flex items-end sm:items-center justify-center p-4 z-[3000]"
        @click.self="dismiss"
      >
        <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" />
        <div
          class="glass-card p-5 w-full max-w-sm relative z-10 mb-20 sm:mb-0 overflow-hidden"
          @click.stop
        >
          <button
            type="button"
            class="absolute right-3 top-3 h-8 w-8 rounded-full bg-white/5 border border-white/10 text-white/60 hover:text-white hover:bg-white/10 transition-colors z-10"
            aria-label="Close companion modal"
            @click="dismiss"
          >
            &times;
          </button>

          <Transition :name="slideName" mode="out-in">
            <!-- Screen 1: get the app -->
            <div v-if="step === 'download'" key="download">
              <div class="flex items-start gap-4 mb-4">
                <div class="w-12 h-12 rounded-xl bg-orange-500/15 border border-orange-500/30 flex items-center justify-center flex-shrink-0">
                  <svg class="w-7 h-7 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                    <rect x="3" y="7" width="18" height="11" rx="3" stroke-width="1.5" />
                    <rect x="7.5" y="10" width="2" height="5" rx="0.5" fill="currentColor" />
                    <rect x="6" y="11.5" width="5" height="2" rx="0.5" fill="currentColor" />
                    <circle cx="16" cy="11" r="1.2" fill="currentColor" />
                    <circle cx="14" cy="13.5" r="1.2" fill="currentColor" />
                  </svg>
                </div>
                <div class="min-w-0 flex-1">
                  <h3 class="text-lg font-semibold text-white mb-1">Remote Companion</h3>
                  <p class="text-sm text-white/60 leading-relaxed">
                    Install the Archipelago companion app on your phone, scan the code, and connect to the same node.
                  </p>
                </div>
              </div>

              <div class="hidden md:flex justify-center mb-4">
                <div class="w-[128px] rounded-2xl border border-white/10 bg-white/[0.03] p-1 overflow-hidden">
                  <img
                    v-if="qrDataUrl"
                    :src="qrDataUrl"
                    alt="Companion app download QR code"
                    class="block w-full max-w-full h-auto rounded-lg bg-white"
                  />
                  <div v-else class="w-full aspect-square rounded-lg bg-white/5"></div>
                </div>
              </div>

              <div class="flex gap-2">
                <a
                  :href="companionDownloadUrl"
                  class="flex-1 inline-flex items-center justify-center rounded-lg bg-orange-500/20 border border-orange-500/30 px-3 py-2.5 text-sm font-medium text-orange-400 hover:bg-orange-500/30 transition-colors text-center"
                  target="_blank"
                  rel="noopener noreferrer"
                  download
                >
                  Download app
                </a>
                <button
                  type="button"
                  class="flex-1 rounded-lg bg-white/5 border border-white/15 px-3 py-2.5 text-sm font-medium text-white/80 hover:bg-white/10 hover:text-white transition-colors"
                  @click="showPairScreen"
                >
                  I've installed it
                </button>
              </div>
              <!-- Which version the Download button installs — read from the
                   metadata staged beside the APK; absent file, absent note -->
              <p v-if="companionVersion" class="text-xs text-white/40 text-center mt-3">
                Version {{ companionVersion.versionName }}<template v-if="companionVersion.versionCode"> (build {{ companionVersion.versionCode }})</template>
              </p>
            </div>

            <!-- Screen 2: pair the app with this node -->
            <div v-else key="pair">
              <div class="flex items-start gap-4 mb-4">
                <div class="w-12 h-12 rounded-xl bg-orange-500/15 border border-orange-500/30 flex items-center justify-center flex-shrink-0">
                  <svg class="w-7 h-7 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                    <rect x="4" y="4" width="7" height="7" rx="1.5" stroke-width="1.5" />
                    <rect x="13" y="4" width="7" height="7" rx="1.5" stroke-width="1.5" />
                    <rect x="4" y="13" width="7" height="7" rx="1.5" stroke-width="1.5" />
                    <path d="M13 13h3v3h-3zM17 17h3v3h-3z" fill="currentColor" stroke="none" />
                  </svg>
                </div>
                <div class="min-w-0 flex-1">
                  <h3 class="text-lg font-semibold text-white mb-1">Connect your app</h3>
                  <p class="text-sm text-white/60 leading-relaxed">
                    In the companion app, choose “Scan Node's QR” and point your phone here — it connects instantly and sets up secure remote access over the mesh.
                  </p>
                </div>
              </div>

              <div class="flex justify-center mb-4">
                <!-- Bigger than the download QR: this one is scanned by the
                     companion app camera, and physical size drives decode. -->
                <div class="w-[192px] rounded-2xl border border-white/10 bg-white/[0.03] p-1 overflow-hidden">
                  <img
                    v-if="pairQrDataUrl"
                    :src="pairQrDataUrl"
                    alt="Companion app pairing QR code"
                    class="block w-full max-w-full h-auto rounded-lg bg-white"
                  />
                  <div v-else class="w-full aspect-square rounded-lg bg-white/5 flex items-center justify-center">
                    <svg v-if="pairLoading" class="w-6 h-6 animate-spin text-white/40" fill="none" viewBox="0 0 24 24"><circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" /><path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" /></svg>
                  </div>
                </div>
              </div>

              <!-- Same-device path: a QR on the phone's own screen can't be
                   scanned, so offer the deep link directly on small screens. -->
              <a
                v-if="pairingUrl"
                :href="pairingUrl"
                class="md:hidden inline-flex w-full items-center justify-center rounded-lg bg-orange-500/20 border border-orange-500/30 px-4 py-2.5 text-sm font-medium text-orange-400 hover:bg-orange-500/30 transition-colors mb-2"
              >
                Open in companion app
              </a>

              <button
                type="button"
                class="w-full py-2.5 rounded-lg bg-white/5 border border-white/15 text-white/80 text-sm font-medium hover:bg-white/10 hover:text-white transition-colors"
                @click="showDownloadScreen"
              >
                Back
              </button>
            </div>
          </Transition>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import * as QRCode from 'qrcode'
import { IS_DEMO, DEMO_PASSWORD } from '@/composables/useDemoIntro'
import { companionIntroRequested } from '@/composables/useCompanionIntro'
import { isCompanionApp } from '@/utils/openExternal'
import { useLoginTransitionStore } from '@/stores/loginTransition'
import { useServerStore } from '@/stores/server'
import { rpcClient } from '@/api/rpc-client'

const STORAGE_KEY = 'neode_companion_intro_seen'
// Absolute URL so the QR works when scanned by a phone (a relative path has no
// host to resolve). Points at the companion APK on the release server's https
// domain (bare-IP origins retired 2026-08-11; /packages/ is proxied to the
// same package host that previously answered on the IP).
// The demo serves the APK from its own public origin instead, so the QR never
// exposes the release-server address.
const DEFAULT_DOWNLOAD_URL = IS_DEMO
  ? `${window.location.origin}/packages/archipelago-companion.apk`
  : 'https://source.archipelago-foundation.org/packages/archipelago-companion.apk'

// Version note for the download step. Read from the node's own copy of the
// metadata (ships in the frontend beside the APK at /packages/), written by
// publish-companion-apk.sh from the same gradle config that built the APK.
// Best-effort: no file, no note.
const companionVersion = ref<{ versionName: string; versionCode: number } | null>(null)
async function loadCompanionVersion() {
  try {
    const res = await fetch('/packages/archipelago-companion.json', { cache: 'no-store' })
    if (!res.ok) return
    const meta = await res.json()
    if (meta && typeof meta.versionName === 'string' && meta.versionName) {
      companionVersion.value = { versionName: meta.versionName, versionCode: Number(meta.versionCode) || 0 }
    }
  } catch { /* metadata is a nicety — the download works without it */ }
}

// Deep-link scheme the companion app registers; carries the server entry the
// app should create (see docs/companion-pairing-qr.md for the contract).
const PAIR_SCHEME = 'archipelago://pair'
const DEMO_SERVER_URL = 'https://demo.archipelago-foundation.org'

// Fallback display name when the node still has the factory server name.
const DEFAULT_PAIR_NAME = 'My Archipelago'

// Device-token entry name; re-minting replaces the previous token so
// re-showing this screen never piles up credentials server-side.
const DEVICE_TOKEN_NAME = 'companion'

const visible = ref(false)
const step = ref<'download' | 'pair'>('download')
const slideName = ref('slide-forward')
const qrDataUrl = ref('')
const pairQrDataUrl = ref('')
const pairingUrl = ref('')
const pairLoading = ref(false)
const companionDownloadUrl = import.meta.env.VITE_COMPANION_APK_URL || DEFAULT_DOWNLOAD_URL

const loginTransition = useLoginTransitionStore()
const serverStore = useServerStore()

// Base delay before the popup may appear, and extra breathing room after the
// dashboard entrance cinematic ends so the popup never cuts into the reveal.
const BASE_DELAY_MS = 5000
const POST_INTRO_GRACE_MS = 2000

let calmTicker: ReturnType<typeof setInterval> | null = null

// Running inside the companion app's own WebView (it injects the JS bridge —
// detected with the canonical helper, not a raw window check, so the gate
// is identical everywhere the question is asked).
// The "get the companion app" pitch is nonsense there — the user is already in
// it. Server management for connected companions lives in the NESMenu instead.
const IN_COMPANION_APP = isCompanionApp()

onMounted(() => {
  if (IN_COMPANION_APP) return
  try {
    if (localStorage.getItem(STORAGE_KEY) !== '1') {
      setTimeout(maybeShow, BASE_DELAY_MS)
    }
  } catch {
    // localStorage unavailable
  }
})

onUnmounted(() => {
  if (calmTicker) clearInterval(calmTicker)
})

function maybeShow() {
  // Show only after the scene has been CONTINUOUSLY calm (no reveal
  // cinematic) for the full grace window. The previous point-in-time check
  // raced a slow-starting reveal — on a cold cache the entrance video can
  // begin buffering after the 5s base delay, so the flag was still false
  // when sampled and the popup cut straight into the cinematic.
  let calmSince = loginTransition.introCinematicPlaying ? null : Date.now()
  calmTicker = setInterval(() => {
    if (loginTransition.introCinematicPlaying) {
      calmSince = null
      return
    }
    if (calmSince === null) calmSince = Date.now()
    if (Date.now() - calmSince >= POST_INTRO_GRACE_MS) {
      if (calmTicker) clearInterval(calmTicker)
      calmTicker = null
      visible.value = true
    }
  }, 250)
}

// Manual open (App Store banner etc.) — ignores the once-per-browser gate.
// The trigger itself is already a no-op inside the companion (useCompanionIntro),
// and this watcher refuses to open there too, so no caller can ever pop the
// install pitch inside the app it installs (#61).
watch(companionIntroRequested, (requested) => {
  if (!requested) return
  companionIntroRequested.value = false
  if (IN_COMPANION_APP) return
  if (calmTicker) {
    clearInterval(calmTicker)
    calmTicker = null
  }
  step.value = 'download'
  visible.value = true
})

watch(visible, async (isVisible) => {
  if (!isVisible) return
  if (!companionVersion.value) void loadCompanionVersion()
  // Generate large and let CSS scale down — at 112px source a ~45-module QR
  // is 2.5px/module, which camera decoders (the companion app included)
  // routinely fail on. 512px keeps every module crisp.
  qrDataUrl.value = await QRCode.toDataURL(companionDownloadUrl, {
    width: 512,
    margin: 2,
    errorCorrectionLevel: 'M',
    color: {
      dark: '#111111',
      light: '#ffffff',
    },
  })
}, { immediate: true, flush: 'post' })

/** Tailscale/CGNAT range 100.64.0.0/10 — reachable only inside the tailnet. */
function isTailnetIp(host: string): boolean {
  const m = host.match(/^100\.(\d{1,3})\.\d{1,3}\.\d{1,3}$/)
  return !!m && Number(m[1]) >= 64 && Number(m[1]) <= 127
}

/**
 * The server URL the companion app should connect to. The demo advertises its
 * public https origin; a real node advertises the browser's own origin ONLY
 * when a phone could plausibly reach it too. Two origins that a phone on the
 * LAN can never dial get substituted with the node's real LAN address:
 *   - localhost/127.0.0.1 (the kiosk browses itself)
 *   - a tailnet 100.x address (operator browsing over Tailscale — a scanned
 *     QR carried one of these on 2026-07-22 and the companion sat there
 *     dialing an IP the phone had no route to)
 *   - a .fips name or mesh ULA (operator browsing over the mesh — a scanned
 *     QR carried npub….fips as fhost on 2026-07-24; Android's system DNS
 *     can't resolve .fips, so the phone's direct dial died and first
 *     connect crawled through anchor discovery instead of the LAN)
 */
async function resolveServerUrl(): Promise<string> {
  if (IS_DEMO) return DEMO_SERVER_URL
  const { hostname, origin } = window.location
  const phoneUnreachable =
    hostname === 'localhost' ||
    hostname === '127.0.0.1' ||
    isTailnetIp(hostname) ||
    hostname.endsWith('.fips') ||
    hostname.includes(':') // IPv6 literal — the node's mesh ULA
  if (!phoneUnreachable) return origin
  try {
    const res = await rpcClient.call<{ mdns_hostname?: string; lan_ip?: string | null }>({
      method: 'system.get-hostname',
    })
    // Prefer the LAN IP (phones resolve it without mDNS support — Android
    // notoriously lacks .local); fall back to the mDNS name.
    if (res?.lan_ip) return `http://${res.lan_ip}`
    if (res?.mdns_hostname) return `http://${res.mdns_hostname}`
  } catch {
    // RPC unavailable — fall through to the (unreachable) origin; the app
    // still lets the user edit the address by hand.
  }
  return origin
}

/**
 * Pairing payload the companion app consumes (docs/companion-pairing-qr.md):
 *   v      contract version (1)
 *   url    where the app connects right now (LAN origin / mDNS)
 *   name   what the server is called in the app
 *   tok    device token — the app logs in with it instantly (real nodes)
 *   pw     shared demo password (demo only)
 *   fnpub  node's FIPS mesh identity — the phone's embedded FIPS dials it
 *   fip    node's fips0 ULA — where the UI stays reachable once meshed
 *   fhost  host the phone dials for the mesh (same host `url` resolved to)
 *   fudp / ftcp   mesh transport ports on fhost
 *
 * Token and mesh params are best-effort: without them the QR still pairs the
 * old way (manual password, LAN only), so a mid-onboarding backend hiccup
 * never produces a dead QR.
 */
async function buildPairingUrl(): Promise<string> {
  const serverUrl = await resolveServerUrl()
  const params = new URLSearchParams({ v: '1', url: serverUrl })

  const name = serverStore.serverName
  params.set('name', !name || name === 'Archipelago' ? DEFAULT_PAIR_NAME : name)

  if (IS_DEMO) {
    // Only the shared demo password ever rides in the QR; real node passwords
    // are never available to the frontend.
    params.set('pw', DEMO_PASSWORD)
    return `${PAIR_SCHEME}?${params.toString()}`
  }

  try {
    const res = await rpcClient.call<{ token?: string }>({
      method: 'auth.createDeviceToken',
      params: { name: DEVICE_TOKEN_NAME },
    })
    if (res?.token) params.set('tok', res.token)
  } catch {
    // Older backend or transient failure — app falls back to password entry.
  }

  try {
    const info = await rpcClient.call<{
      npub?: string
      ula?: string | null
      udp_port?: number
      tcp_port?: number
      anchors?: { npub: string; addr: string; transport: string }[]
    }>({ method: 'fips.pair-info' })
    if (info?.npub) {
      params.set('fnpub', info.npub)
      if (info.ula) params.set('fip', info.ula)
      try {
        params.set('fhost', new URL(serverUrl).hostname)
      } catch {
        params.set('fhost', window.location.hostname)
      }
      if (info.udp_port) params.set('fudp', String(info.udp_port))
      if (info.tcp_port) params.set('ftcp', String(info.tcp_port))
      // Rendezvous anchors (compact: npub@addr/transport, comma-joined).
      // FIRST entry is the paired node ITSELF: the phone peers with it by
      // npub (the fhost addr is only a dial hint), so LAN contact is direct
      // p2p over FIPS and survives DHCP renumbering; the node's public
      // anchors follow for reaching it away from home. Cap keeps the QR at a
      // camera-friendly density; the node lists its most reachable anchors
      // first.
      const selfAnchor = info.tcp_port
        ? [{ npub: info.npub, addr: `${params.get('fhost')}:${info.tcp_port}`, transport: 'tcp' }]
        : []
      // HARD CAP at 2 anchors: every extra npub adds ~100 URL-encoded chars
      // and pushed the QR past what phone cameras decode off a screen
      // (3 anchors ≈ 600 chars ≈ QR v23 — observed unscannable 2026-07-23).
      // Self + one public rendezvous is all the phone needs; prefer the
      // Archipelago-operated vps2 anchor as the public one.
      const others = (info.anchors || []).filter((a) => a.npub !== info.npub)
      const publicAnchor =
        others.find((a) => a.addr.startsWith('146.59.87.168')) ?? others[0]
      const anchors = [...selfAnchor, ...(publicAnchor ? [publicAnchor] : [])].slice(0, 2)
      if (anchors.length) {
        params.set(
          'fanchors',
          anchors.map((a) => `${a.npub}@${a.addr}/${a.transport}`).join(','),
        )
      }
    }
  } catch {
    // FIPS not provisioned yet — LAN pairing still works; the app can mesh
    // later from a re-scan.
  }

  return `${PAIR_SCHEME}?${params.toString()}`
}

async function showPairScreen() {
  slideName.value = 'slide-forward'
  step.value = 'pair'
  if (pairQrDataUrl.value || pairLoading.value) return
  pairLoading.value = true
  try {
    pairingUrl.value = await buildPairingUrl()
    // Large source + a real quiet zone; this QR is scanned by the companion
    // app's camera, so give it every advantage (see download QR note above).
    // EC level L: the payload is long (token + npub + anchors) and screen
    // scans don't suffer the damage EC-M protects against — L drops the
    // module count a full version tier, which is what makes it scannable.
    pairQrDataUrl.value = await QRCode.toDataURL(pairingUrl.value, {
      width: 768,
      margin: 3,
      errorCorrectionLevel: 'L',
      color: {
        dark: '#111111',
        light: '#ffffff',
      },
    })
  } finally {
    pairLoading.value = false
  }
}

function showDownloadScreen() {
  slideName.value = 'slide-back'
  step.value = 'download'
}

function dismiss() {
  visible.value = false
  step.value = 'download'
  try {
    localStorage.setItem(STORAGE_KEY, '1')
  } catch {
    // ignore
  }
}
</script>

<style scoped>
.overlay-fade-enter-active { transition: opacity 0.3s ease; }
.overlay-fade-leave-active { transition: opacity 0.2s ease; }
.overlay-fade-enter-from,
.overlay-fade-leave-to { opacity: 0; }
.overlay-fade-enter-active .glass-card { transition: transform 0.3s ease; }
.overlay-fade-enter-from .glass-card { transform: translateY(20px); }

/* Horizontal slide between the download and pairing screens */
.slide-forward-enter-active,
.slide-forward-leave-active,
.slide-back-enter-active,
.slide-back-leave-active { transition: transform 0.25s ease, opacity 0.25s ease; }
.slide-forward-enter-from { transform: translateX(40px); opacity: 0; }
.slide-forward-leave-to { transform: translateX(-40px); opacity: 0; }
.slide-back-enter-from { transform: translateX(-40px); opacity: 0; }
.slide-back-leave-to { transform: translateX(40px); opacity: 0; }
</style>
