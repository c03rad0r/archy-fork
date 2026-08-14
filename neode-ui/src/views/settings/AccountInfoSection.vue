<script setup lang="ts">
import { computed, ref, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useAppStore } from '@/stores/app'
import ControllerIndicator from '@/components/ControllerIndicator.vue'
import { rpcClient } from '@/api/rpc-client'
import { useBodyScrollLock } from '@/composables/useBodyScrollLock'

const { t } = useI18n()
const store = useAppStore()

// Server name
const serverName = computed(() => store.serverName)
const editingServerName = ref(false)
const serverNameDraft = ref('')
const serverNameInput = ref<HTMLInputElement | null>(null)
const serverNameWarning = ref('')

function startEditServerName() {
  serverNameDraft.value = serverName.value
  editingServerName.value = true
  nextTick(() => serverNameInput.value?.select())
}

async function saveServerName() {
  const name = serverNameDraft.value.trim()
  serverNameWarning.value = ''
  if (!name || name === serverName.value) {
    editingServerName.value = false
    return
  }
  try {
    const result = await rpcClient.call<{ hostname_error?: string | null }>({ method: 'server.set-name', params: { name } })
    store.updateServerName(name)
    if (result.hostname_error) {
      serverNameWarning.value = `Display name saved, but hostname update failed: ${result.hostname_error}`
    }
  } catch (e) {
    if (import.meta.env.DEV) console.error('Failed to rename server:', e)
  }
  editingServerName.value = false
}

// Version & release notes
const version = computed(() => store.serverInfo?.version || '0.0.0')
const showReleaseNotes = ref(false)
useBodyScrollLock(showReleaseNotes)

// Identity
const serverTorAddressFromStore = computed(() => store.serverInfo?.['tor-address'] || null)
const torAddressFromRpc = ref<string | null>(null)
const serverTorAddress = computed(() => serverTorAddressFromStore.value || torAddressFromRpc.value)
// Fallback DID fetched from the backend when localStorage doesn't have one
// (e.g. a browser/node where onboarding never stored `neode_did`).
const didFromRpc = ref<string | null>(null)
const userDid = computed(() => {
  try {
    return localStorage.getItem('neode_did') || didFromRpc.value
  } catch {
    return didFromRpc.value
  }
})
// The node's seed-derived Nostr public key (npub), fetched from the backend.
const userNpub = ref<string | null>(null)
const copiedNpub = ref(false)

const copiedOnion = ref(false)
const copiedDid = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

// Location sharing — opt-in only, off by default. Lets this node's own
// position appear on OTHER trusted federation peers' Mesh Map (with the
// Archy logo marker), the same way a mesh radio peer's position shows up.
// Backed by the store's serverInfo (already synced live over the WS), not
// a separate fetch, so it stays in sync with any other tab/session too.
const shareLocation = computed(() => !!store.serverInfo?.['share-location'])
const savedLat = computed(() => store.serverInfo?.lat ?? null)
const savedLon = computed(() => store.serverInfo?.lon ?? null)
const locationSaving = ref(false)
const locationError = ref('')

async function useCurrentLocation() {
  locationError.value = ''
  if (!navigator.geolocation) {
    locationError.value = 'Geolocation not supported by this browser'
    return
  }
  locationSaving.value = true
  navigator.geolocation.getCurrentPosition(
    async (pos) => {
      await saveLocation(pos.coords.latitude, pos.coords.longitude, shareLocation.value)
      locationSaving.value = false
    },
    (err) => {
      locationError.value = err.code === 1 ? 'Location permission denied' : err.message
      locationSaving.value = false
    },
    { enableHighAccuracy: true, timeout: 15000 },
  )
}

async function toggleShareLocation() {
  const next = !shareLocation.value
  await saveLocation(savedLat.value, savedLon.value, next)
}

async function saveLocation(lat: number | null, lon: number | null, share: boolean) {
  try {
    locationSaving.value = true
    await rpcClient.call({ method: 'server.set-location', params: { lat, lon, share } })
    // Optimistic update — the next WS state push confirms it, but no need
    // to wait for that round-trip to reflect the change in the toggle/coords.
    if (store.serverInfo) {
      store.serverInfo.lat = lat
      store.serverInfo.lon = lon
      store.serverInfo['share-location'] = share
    }
  } catch (e) {
    locationError.value = e instanceof Error ? e.message : 'Failed to save location'
  } finally {
    locationSaving.value = false
  }
}

// mDNS hostname — HTTPS (even self-signed) is required for mic/camera access
// (getUserMedia refuses plain HTTP outside localhost); surface both so users
// know where to go for features that need it, without forcing HTTPS on anyone.
const mdnsHostname = ref<string | null>(null)
const httpsUrl = computed(() => (mdnsHostname.value ? `https://${mdnsHostname.value}` : null))
const httpUrl = computed(() => (mdnsHostname.value ? `http://${mdnsHostname.value}` : null))
const copiedHttps = ref(false)
async function copyHttpsUrl() {
  if (!httpsUrl.value) return
  try {
    await navigator.clipboard.writeText(httpsUrl.value)
    copiedHttps.value = true
    setTimeout(() => { copiedHttps.value = false }, 2000)
  } catch { /* unavailable */ }
}

async function copyOnionAddress() {
  const addr = serverTorAddress.value
  if (!addr) return
  try {
    await navigator.clipboard.writeText(addr)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = addr
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
  copiedOnion.value = true
  if (copiedTimer) clearTimeout(copiedTimer)
  copiedTimer = setTimeout(() => { copiedOnion.value = false }, 2000)
}

async function copyDid() {
  if (!userDid.value) return
  try {
    await navigator.clipboard.writeText(userDid.value)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = userDid.value
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
  copiedDid.value = true
  setTimeout(() => { copiedDid.value = false }, 2000)
}

async function copyNpub() {
  if (!userNpub.value) return
  try {
    await navigator.clipboard.writeText(userNpub.value)
  } catch {
    return
  }
  copiedNpub.value = true
  setTimeout(() => { copiedNpub.value = false }, 2000)
}

// Load Tor address on mount if not in store
async function init() {
  if (!serverTorAddressFromStore.value) {
    try {
      const res = await rpcClient.getTorAddress()
      torAddressFromRpc.value = res.tor_address ?? null
    } catch (e) {
      if (import.meta.env.DEV) console.warn('Tor address may not be available yet', e)
    }
  }
  // DID: fall back to the node.did RPC when localStorage doesn't have one, so
  // the Identity card shows the DID on every node (not just ones where the
  // browser cached it during onboarding).
  let storedDid: string | null = null
  try { storedDid = localStorage.getItem('neode_did') } catch { /* unavailable */ }
  if (!storedDid) {
    try {
      const res = await rpcClient.call<{ did?: string }>({ method: 'node.did' })
      if (res?.did) {
        didFromRpc.value = res.did
        try { localStorage.setItem('neode_did', res.did) } catch { /* unavailable */ }
      }
    } catch (e) {
      if (import.meta.env.DEV) console.warn('node.did unavailable', e)
    }
  }
  // The node's seed-derived Nostr public key (npub) for the Identity card.
  try {
    const res = await rpcClient.call<{ nostr_npub?: string }>({ method: 'node.nostr-pubkey' })
    if (res?.nostr_npub) userNpub.value = res.nostr_npub
  } catch (e) {
    if (import.meta.env.DEV) console.warn('node.nostr-pubkey unavailable', e)
  }
  // mDNS hostname for the "Access this node" card.
  try {
    const res = await rpcClient.call<{ mdns_hostname?: string }>({ method: 'system.get-hostname' })
    if (res?.mdns_hostname) mdnsHostname.value = res.mdns_hostname
  } catch (e) {
    if (import.meta.env.DEV) console.warn('system.get-hostname unavailable', e)
  }
}
init()
</script>

<template>
  <!-- Controller indicator - Mobile only -->
  <div class="md:hidden mb-4">
    <ControllerIndicator />
  </div>

  <!-- Info Grid -->
  <div class="grid grid-cols-1 md:grid-cols-2 gap-4 mb-6">
    <!-- Server Name Card (editable) — container: Enter to edit, Enter to save, Escape to exit -->
    <div data-controller-container tabindex="0" class="bg-black/20 rounded-xl px-5 py-4 border border-white/10 transition-all hover:-translate-y-1">
      <div class="flex items-center gap-3 mb-2">
        <svg class="w-5 h-5 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
        </svg>
        <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">{{ t('settings.serverName') }}</p>
      </div>
      <div v-if="editingServerName" class="flex items-center gap-2">
        <input
          ref="serverNameInput"
          v-model="serverNameDraft"
          type="text"
          maxlength="64"
          class="flex-1 px-3 py-1.5 bg-white/10 border border-white/20 rounded-lg text-white text-lg font-semibold focus:outline-none focus:border-white/40 transition-colors"
          @keydown.enter="saveServerName"
          @keydown.escape="editingServerName = false"
        />
        <button
          class="px-3 py-1.5 bg-white/10 border border-white/20 rounded-lg text-white/70 hover:text-white hover:bg-white/15 transition-colors text-sm"
          @click="saveServerName"
        >Save</button>
        <button
          class="px-3 py-1.5 text-white/50 hover:text-white/70 transition-colors text-sm"
          @click="editingServerName = false"
        >Cancel</button>
      </div>
      <div v-else class="flex items-center gap-2 group cursor-pointer" @click="startEditServerName">
        <p class="text-lg font-semibold text-white/95">{{ serverName }}</p>
        <svg class="w-4 h-4 text-white/30 group-hover:text-white/60 transition-colors" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
        </svg>
      </div>
      <p v-if="serverNameWarning" class="mt-2 text-xs text-yellow-300">{{ serverNameWarning }}</p>
    </div>

    <!-- Version Card -->
    <div class="bg-black/20 rounded-xl px-5 py-4 border border-white/10">
      <div class="flex items-center gap-3 mb-2">
        <svg class="w-5 h-5 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z" />
        </svg>
        <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">{{ t('common.version') }}</p>
      </div>
      <div class="flex items-center justify-between">
        <p class="text-lg font-semibold text-white/95">{{ version }}</p>
        <button
          @click="showReleaseNotes = true"
          class="glass-button px-3 py-1.5 text-xs"
        >What's New</button>
      </div>
    </div>

    <!-- Access This Node Card — local hostname + HTTP/HTTPS. HTTPS (even
         self-signed) is needed for mic/camera access on some features; never
         forced, just surfaced so people who need it know where to go. -->
    <div v-if="mdnsHostname" class="bg-black/20 rounded-xl px-5 py-4 border border-white/10">
      <div class="flex items-center gap-3 mb-2">
        <svg class="w-5 h-5 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5.636 18.364a9 9 0 010-12.728m12.728 0a9 9 0 010 12.728m-9.9-2.829a5 5 0 010-7.07m7.072 0a5 5 0 010 7.07M13 12a1 1 0 11-2 0 1 1 0 012 0z" />
        </svg>
        <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">Access on this network</p>
      </div>
      <p class="text-lg font-semibold text-white/95 mb-1">{{ mdnsHostname }}</p>
      <div class="flex items-center gap-2 text-xs text-white/50">
        <a :href="httpUrl!" class="hover:text-white/80 transition-colors underline">http</a>
        <span>·</span>
        <a :href="httpsUrl!" class="hover:text-white/80 transition-colors underline">https</a>
        <span class="text-white/30">(needed for mic/camera features)</span>
        <button @click="copyHttpsUrl" class="ml-auto text-white/40 hover:text-white/70 transition-colors">
          {{ copiedHttps ? 'Copied!' : 'Copy HTTPS link' }}
        </button>
      </div>
    </div>

    <!-- Location Sharing Card — opt-in, off by default. Puts this node on
         OTHER trusted peers' Mesh Map with the Archy logo marker. -->
    <div class="bg-black/20 rounded-xl px-5 py-4 border border-white/10">
      <div class="flex items-center gap-3 mb-2">
        <svg class="w-5 h-5 text-white/70" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 11a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">Share Location</p>
      </div>
      <div class="flex items-center justify-between gap-3 mb-1">
        <p class="text-sm text-white/70">Show this node on trusted peers' Mesh Map</p>
        <button
          class="glass-toggle"
          :class="{ active: shareLocation }"
          role="switch"
          :aria-checked="shareLocation"
          :disabled="locationSaving"
          @click="toggleShareLocation"
        >
          <span class="glass-toggle-knob" />
        </button>
      </div>
      <div class="flex items-center gap-2 text-xs text-white/50 mt-2">
        <span v-if="savedLat !== null && savedLon !== null">{{ savedLat.toFixed(3) }}, {{ savedLon.toFixed(3) }}</span>
        <span v-else class="text-white/30">No location set</span>
        <button
          class="ml-auto glass-button px-3 py-1 text-xs"
          :disabled="locationSaving"
          @click="useCurrentLocation"
        >{{ locationSaving ? 'Locating…' : 'Use current location' }}</button>
      </div>
      <p v-if="locationError" class="mt-2 text-xs text-yellow-300">{{ locationError }}</p>
    </div>

    <!-- Release Notes Modal -->
    <Teleport to="body">
      <Transition name="modal">
        <div v-if="showReleaseNotes" class="fixed inset-0 z-[3000] flex items-center justify-center p-4" @click="showReleaseNotes = false">
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
          <div @click.stop class="glass-card p-6 max-w-lg w-full relative z-10 flex flex-col" style="max-height: 85vh">
            <div class="flex items-start justify-between gap-4 mb-5 shrink-0">
              <h3 class="text-xl font-semibold text-white">What's New</h3>
              <button @click="showReleaseNotes = false" class="p-2 rounded-lg hover:bg-white/10 text-white/70 hover:text-white transition-colors" aria-label="Close">
                <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
              </button>
            </div>
            <div class="overflow-y-auto flex-1 min-h-0 space-y-6 pr-1">
              <!-- v1.8.2-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.2-alpha</span>
                  <span class="text-xs text-white/40">August 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**An app that can't be shown inside the dashboard now becomes a tab app by itself.** A few apps refuse to render inside another page no matter what — they break out with their own code or insist on owning the whole browser window. Opening one used to mean staring at a grey pane. Now the dashboard notices, offers the app in its own tab, and remembers: from then on that app's button opens a tab directly (with the little launch icon that tab apps carry), first click, every time. If a later update makes the app embeddable after all, the dashboard notices that too and goes back to embedding it.</p>
                  <p>**The logo emblem got its glossy black paint finish — properly this time.** The circle behind the A on the screensaver, intro, and login now wears a deep wet-paint look: warm light blooming from the top edge, fine grain so the dark tones stay smooth instead of banding, and no more ring border. (An earlier rougher version of this experiment briefly shipped by accident and then vanished depending on which screen you were on — this is the finished, deliberate one, everywhere.)</p>
                  <p>**New app icons now match the store's look, on every screen.** Alby Hub and phoenixd arrived with edge-to-edge logos that ignored the breathing room every other app icon has, and the app detail page skipped the icon backdrop entirely. Both icons are re-set on the standard canvas, the detail page now applies the same icon treatment as the store tiles, and app developers get a one-command tool that puts any logo onto the house canvas automatically.</p>
                </div>
              </div>
              <!-- v1.8.1-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.1-alpha</span>
                  <span class="text-xs text-white/40">August 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Apps that refused to open inside the dashboard now embed like everything else.** Some apps ship browser headers that forbid being shown inside another page — correct hardening on the open web, but inside Archipelago it produced a dead grey pane when you opened them from My Apps (Alby Hub was the first to hit it). The app gate, which already checks your login on every request to an app, now removes just those framing headers on the way through; each app's own content-security rules pass through untouched. No more per-app proxy workarounds.</p>
                  <p>**The network map no longer freezes kiosk TVs.** The animated federation map at 4K was too much for the deliberately conservative graphics settings the on-screen display used on every machine — settings chosen years back to stop audio crackle on much older hardware. Two fixes: on kiosk screens the map now opens in its flat 2D view (the 3D globe is one tap away, and remembered) and animates at half rate — invisible from the couch, half the work. And the display itself now recognizes what machine it runs on: older kiosk boxes keep the proven careful settings, modern ones finally get real GPU rendering.</p>
                  <p>**New Settings → Display → Graphics choice for the on-screen display.** Auto (recommended) picks the right rendering mode for the machine by itself; Compatibility forces the most conservative mode if a screen ever stutters, tears, or crackles; Quality forces full GPU rendering on hardware the automatic detection doesn't recognize. Changing it restarts the on-screen display, like the size presets.</p>
                </div>
              </div>
              <!-- v1.8.0-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.0-alpha</span>
                  <span class="text-xs text-white/40">August 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Archipelago is now open source.** The full source code of the node you are running — the orchestrator, the dashboard, the app platform, the mesh, the release tooling — is published for anyone to read, build and audit at source.archipelago-foundation.org/lfg2025/archy. A node that holds your money, your files and your communications should not ask to be taken on faith: from this release onward you, or anyone you trust, can see exactly what it does and follow every change we make in the open.</p>
                  <p>**Installing an update is reliable again, and tells you what happened when it isn't.** Some nodes could download an update but never apply it — the button stayed on "Install", and no amount of retrying worked. The cause: applying the update consumed the downloaded files as it went, so if any one step hit a snag partway through, the leftover files were incomplete and every later attempt failed the safety re-check forever, needing a technician to recover. Applying no longer consumes the download — a failed apply can always be retried from the same files — and the pieces are now applied in a fixed order with the program itself last, so a hiccup can't leave a half-swapped node. When an apply does fail, the screen now shows the real reason and what to do ("download the update again"), and offers Download again instead of a dead "Install" button, rather than a generic "it failed".</p>
                  <p>**Video on the kiosk stops tearing.** The kiosk's display had no vertical sync at all, so fast motion — IndeedHub films especially — showed horizontal tearing lines. The display driver now syncs every frame to the panel (no extra hardware needed, existing kiosks pick it up with this update), and on machines with a GPU, video decoding moves off the CPU onto the video hardware — smoother playback that also leaves more headroom for audio, not less.</p>
                  <p>**The Back button finally does what you expect.** Pressing Back — the mouse's side button on a kiosk, a swipe on a phone, the toolbar button in any browser — used to navigate the screen underneath an open window, or leave the dashboard entirely. Back now closes the topmost open window first, one per press, exactly like a native app; closing a window yourself never leaves a phantom entry that makes you press Back twice.</p>
                  <p>**No more bare IP addresses in your update or app-registry settings.** The update mirrors and the app registry each listed the same server twice — once by its proper name, once as a raw http://146… address left over from before the domain existed. The raw-address entries are retired: new nodes never see them, and existing nodes clean them out of their saved lists automatically on the next read. Everything now goes through the named, TLS-protected origin — which was always the same machine.</p>
                  <p>**The phone companion app downloads over the proper domain.** The download QR pointed at a raw address over plain HTTP; it now points at the same file on the https domain. Scanning it gets you an encrypted download from a named server.</p>
                  <p>**The Receive window now tells you when the money is on its way.** Previously it showed a QR code and left you to check elsewhere whether anything happened. Now, the moment the sender's transaction is broadcast, the QR gives way to a clock: the amount, the transaction ID (tap to copy), and a note that the funds arrive on their own — with a single Done button. If you keep the window open, the clock becomes a green check at the first confirmation. Verified live on a real node: payment detected within seconds of broadcast.</p>
                </div>
              </div>
              <!-- v1.7.129-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.129-alpha</span>
                  <span class="text-xs text-white/40">August 10, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Every app is now supervised the same way — the last stragglers moved under systemd.** Five apps (Jellyfin, Nextcloud, Home Assistant, Uptime Kuma, Vaultwarden) still ran outside the node's per-app service management for a technical reason: their networking style died with whatever process started it, so they were kept alive by a separate workaround. That workaround is retired: these apps now migrate themselves onto the same managed units as everything else — own service, restart-on-anything, a ten-second breather between restarts so their networking can release its ports cleanly. The migration happens automatically on the node's next housekeeping pass, touches no app data, and was watched live on a real node: both test apps moved over on the first pass and came back healthy.</p>
                  <p>**Leftover companion screens are cleaned up again — driven by real records this time.** When an app is uninstalled, its helper screen (the UI tile that fronts it) should go too. That cleanup was switched off in an earlier release after it wrongly removed the Bitcoin screen from a node whose Bitcoin was installed — it had been guessing "installed" from what happened to be running, and a separate bug made a running app look absent. The node now keeps a durable record of what you have installed, written at install time and cleared only by a real uninstall, and the cleanup consults only that record. If the record can't be read, the cleanup does nothing at all — "I couldn't check" is never treated as "nothing is installed" — and a helper must be orphaned for a sustained period before it is touched.</p>
                  <p>**A warning that fired every minute on every node is gone.** The app catalog and the node disagreed about where Grafana's software comes from, so the node ignored the catalog's answer and logged a complaint roughly every 75 seconds, forever. The catalog was right — Grafana is served from the fleet's own registry, like Bitcoin Knots — and the node's records now agree with it.</p>
                  <p>**The federation map became a real map.** The network view is now a 3D orbital scene of your federation — nodes as a point-cloud globe with calm motion, auto-fit centring, and a 2D top-down toggle that portrait and mobile screens use by default, with the scene filling the viewport instead of sitting in a letterbox. Inbound peer requests appear live on the map as blinking nodes you can accept or reject in place, and revisiting the view no longer replays the whole intro — the scene updates in place.</p>
                  <p>**An app that's mid-restart shows a page that says so — and comes back by itself.** When an app's screen was briefly unreachable behind the gate, the browser got a bare error; it now gets a named page for that app that retries on its own until the app answers.</p>
                  <p>Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release; the supervision migration and the cleanup re-enable were verified live on one node (both apps migrated and healthy, cleanup correctly idle).</p>
                </div>
              </div>
              <!-- v1.7.128-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.128-alpha</span>
                  <span class="text-xs text-white/40">August 10, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**The discovery list stops showing ghosts.** Every reinstall of a node mints a new discovery identity, and the old identity's announcement could never be removed from the public relays — nothing holds its key anymore — so the "Discoverable nodes" list slowly filled with entries that led nowhere. Announcements now expire: your node re-announces itself twice a day, each announcement carries a 48-hour expiry that relays honour, anything older than that is ignored when reading, and switching discovery off — or factory-resetting the node — actively overwrites the announcement before it can become a ghost. Old ghosts from earlier versions stop being shown immediately and age off the relays on their own.</p>
                  <p>**You can name your node when you make it discoverable.** Turning discovery on now asks for an optional display name — it travels inside the public announcement, so other nodes' discovery lists show "Dorian's basement node" instead of a bare npub. The name is public by construction, capped at 32 characters, and blank is fine: you list as npub only. Toggling discovery off and on remembers the name; you can clear it the same way you set it.</p>
                  <p>**The discoverability panel now shows what the network actually sees: your node's npub.** It previously showed your Tor address — which is precisely the thing the announcement never contains (your address stays private until you approve a peer). The npub, the identity other nodes discover you by and send peering requests to, is now displayed there with a copy button.</p>
                  <p>**The seed screen stops flashing while the node starts.** During first boot, the lock icon and "server starting" text blinked in and out every few seconds while the node came up — each silent retry briefly emptied the screen. The waiting state now holds steady, with its elapsed timer, until the node answers.</p>
                  <p>**A node that already has an identity now explains itself on the seed screen.** Reaching seed creation on a provisioned node used to surface a developer message about "the authenticated system.factory-reset". It now says what you can actually do: sign in normally, or factory-reset the node from Settings to start it over.</p>
                  <p>Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release; the changes were verified by operator UAT on a live node.</p>
                </div>
              </div>
              <!-- v1.7.127-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.127-alpha</span>
                  <span class="text-xs text-white/40">August 9, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Your node now has its own assistant.** This is the first release to ship AIUI: a conversational screen that can answer from your node's own content — your films, music and files come first, the open web second — and can act on the node itself: install or remove an app, check what's running, or queue up your media, all through a fixed list of vetted actions rather than free rein. It is off-limits to your data until you say otherwise: every data category starts closed, grants are made in Settings → AI Data Access and live on the node itself, and anything that changes the node asks you to confirm in the dashboard's own chrome first — a declined action stays declined. What leaves the node is screened: your API key is stored encrypted and never written in plain text, credential-shaped strings are scrubbed from app logs before the model sees them, your public address and Wi-Fi name are stripped from network answers, web search is gated behind your login session, and cloud-bound text passes a secret scan on the way out. Three model backends are supported — Anthropic's API, a local Ollama, and pay-per-use Routstr with a hard prepaid budget ceiling — and mesh peers can reach the same loop with !ai.</p>
                  <p>**Tor now tells you the truth, heals itself, and the Restart button really restarts it.** Three nodes ran for days with Tor completely dead while the dashboard said "Connected" — the indicator was reading a leftover address file, not the daemon, and the restart button reported success without checking. The cause was a configuration line Tor can never bind on our systems; a node could re-break itself from a single settings change. The node now refuses to write that line, checks Tor with a real connection instead of a leftover file, repairs its own Tor configuration at every start, and the Restart button only claims success once Tor is actually answering. Onion addresses that had silently never been published (BTCPay's included) come back with it.</p>
                  <p>**Inviting another node as Trusted works again — on every node.** Generating a Trusted invite, or promoting a peer from the dropdown, silently failed everywhere: the security prompt that asks for your node password could never appear, because the message requesting it was being scrubbed out of the reply on its way to your browser. The prompt now opens, and if a trust change fails, the error appears inside the window you are looking at instead of hidden behind it.</p>
                  <p>**The mempool explorer actually connects now.** The page loaded but sat empty forever. Three separate causes stacked up: the block index had spent days rebuilding without anything saying so, and then two different layers of the node's plumbing were dropping the live-data connection the page depends on — so everything reported healthy while your screen showed nothing. All three are fixed, and the node's own health checks now test the real connection a browser makes, so this cannot pass unnoticed again.</p>
                  <p>**Apps no longer vanish after stopping cleanly.** A stopped app's container is deleted by design, but the restart policy meant an app that exited cleanly was never brought back — it simply disappeared until reinstalled. Backends now restart in every case, the node remembers what you have installed so a missing app is recreated rather than forgotten, and this release repairs the incorrect policy on apps installed by earlier versions.</p>
                  <p>**Your Bitcoin node will not silently change software versions anymore.** "Latest" previously meant different things in different places — one path installed a newer build that deliberately halts until you make a network-rules decision, which froze one node's sync at a fixed block while it reported itself fully synced. Bitcoin Knots is now pinned to an explicit, known-good version; changing it is a decision you make, never a side effect of an update.</p>
                  <p>**Smaller fixes:** the AI data-access settings now say plainly which categories the assistant can see but not act on; the transactions window's tab bar is transparent glass instead of a black block; BTCPay logins no longer fail with a server error when the node is under heavy load right at that moment.</p>
                  <p>**You can now replace your Lightning connection keys from Settings, without touching a terminal.** The tokens wallet apps like Zeus use to reach your node are bearer keys: anything that has ever seen one can spend from your node until they are replaced, and there is no way to cancel one individually. Replacing them was previously a script you had to SSH in and run, which in practice meant it never happened. Settings → Lightning credentials now shows when yours were issued, which node they belong to and how many channels must survive, then does the whole job behind your node password — with a step-by-step progress list, and a refusal to call it a success unless it has confirmed your node identity and every channel came back. Your coins and channels are not touched: nothing is closed, and the wallet is never re-created. Afterwards you re-pair Zeus by scanning the Lightning app's QR code again.</p>
                  <p>**Replacing those keys no longer silently breaks BTCPay Server.** BTCPay holds its own copy of the key, and that copy cannot repair itself — so a node that replaced its keys ended up with BTCPay running, healthy, and unable to take a single Lightning payment, with nothing anywhere saying why. The dashboard now updates BTCPay's copy as part of the run and restarts it around its existing data, and the Settings screen warns you if it finds a node already stuck in that state. The command-line script fixes the same gap.</p>
                  <p>**Lightning stops getting stuck locked on a busy node.** Lightning opens its databases before it will accept the password that unlocks the wallet, and on a loaded node that took nearly three minutes — longer than the node was willing to wait. Giving up restarted Lightning, which started the slow open again, so the wallet stayed locked forever and everything depending on it stayed broken. The node now waits as long as it takes. A genuinely wrong password still fails immediately.</p>
                  <p>Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The full 5x real-node lifecycle gate was not run for this release; reboot survival was verified directly on a live node — all installed apps returned after a cold reboot, uninstalled apps stayed gone, and restart policy was confirmed on every managed unit.</p>
                </div>
              </div>
              <!-- v1.7.126-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.126-alpha</span>
                  <span class="text-xs text-white/40">August 7, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>The most important fix in this release: the update button could take you backwards onto a version withdrawn for a security hole.</strong> BTCPay Server published 2.4.2 to close a flaw that was being actively exploited — a way past two-factor authentication. Nodes that had already moved to 2.4.2 were then shown an "Update" button offering 2.3.9, the very release being withdrawn, and taking it would have rolled the node back onto the vulnerable version. The cause was that the node only asked whether the two version numbers differed, never which was newer, so any stale record anywhere could present a rollback as an upgrade. It now refuses to offer a lower version as an update, so a stale record fails safe instead of becoming a trap. BTCPay itself is on 2.4.2, and every place that still named the old version — including the fallback installer, which would have installed it outright — has been corrected.</p>
                  <p><strong>An app now reports its own version, not a helper's.</strong> Where an app is made of several parts, the node could read the version of the wrong part: BTCPay showed as "15.17", which is the version of its database, while offering an update to 2.4.2. That is the number update decisions are made from, so a nonsensical pair was being presented as a legitimate upgrade. When the node cannot identify an app's own container it now says so rather than guessing at a neighbour.</p>
                  <p><strong>Your node issues its own certificate, so apps stop being flagged as insecure.</strong> Each node now has its own certificate authority, with a one-step install from Settings, and app screens are served over the same secure connection as the dashboard rather than dropping back to an unprotected one. Apps answer on both the secure and plain address on the same port, so nothing that worked before stops working.</p>
                  <p><strong>An app that is still starting says "starting".</strong> It previously reported "App not reachable", which reads as a failure when the app is simply warming up.</p>
                  <p><strong>Updates and app downloads now come from a proper domain name.</strong> They previously used a bare numeric address over an unprotected connection. Downloads are now encrypted in transit, and the old address is kept as an automatic fallback for nodes whose clock or name lookup is off — the signature, not the address, is what makes either source safe.</p>
                  <p>Also in this release: the tool app developers run to check their app description no longer rejects every valid file, and the node's own security audit — which had been reporting all-clear — now actually inspects the files where credentials had been sitting.</p>
                  <p>Housekeeping, disclosed rather than buried: this release removes Archipelago's own infrastructure details from the published source ahead of the code being opened to the public. No behaviour changes for your node.</p>
                  <p>Known gaps, unchanged from the last release: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.</p>
                </div>
              </div>

              <!-- v1.7.125-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.125-alpha</span>
                  <span class="text-xs text-white/40">August 6, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>The Lightning, Bitcoin, Electrum and mesh screens work again behind the login gate.</strong> Since the gate went up, those screens would load their frame and then show every number as unreachable. The gate was deliberately hiding your login from the apps it protects — right for third-party apps, wrong for the node's own screens, which need that login to fetch your data. The gate now removes only its own credential and the node's own screens explicitly receive yours. The same mistake was also quietly signing you out of apps with their own logins — Vaultwarden, Nextcloud, Gitea — on every single request; that stops too.</p>
                  <p><strong>IndeeHub heals itself.</strong> Three separate faults: its database helper was recreated with permissions too tight to read its own files (it had crashed and restarted about ten thousand times on one node); on another node two of its seven parts could never be recreated at all because of how the node asked for their storage — the node would remove the old part and then fail to build its replacement, leaving the app half-missing forever; and a regenerated password could lock the app out of a database that keeps the original. All three are fixed, and the storage fault fixes the same trap for every future multi-part app.</p>
                  <p><strong>Send and Receive open clean every time.</strong> Whatever you typed last — an address, an amount, and above all an armed "send all funds" toggle — no longer quietly carries over into the next payment. And choosing "send all funds" now shows the amount being swept instead of a confusing 0.</p>
                  <p><strong>The camera scanner option no longer vanishes on desktop.</strong> Browsers only allow the live camera on secure (HTTPS) pages, and the scan window used to silently hide the camera choice on plain connections, which read as "the scanner is gone". The option now stays visible and explains itself, and the photo and paste routes always work. The companion app's built-in scanner is untouched.</p>
                  <p>Also: the app login page uses the Archipelago mark and stays centred on phones with the keyboard open, app icons in the install window are no longer cropped, and when the node fails to build a container it now records the actual reason instead of a one-line stub that hid the cause of the IndeeHub fault for days.</p>
                </div>
              </div>
              <!-- v1.7.124-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.124-alpha</span>
                  <span class="text-xs text-white/40">August 5, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>The most important fix here: some nodes were left switched off by their own update, and could not switch themselves back on.</strong> The node replaces its program and then exits, expecting the system to start it again — but nodes installed from older images carried a setting that only restarts the program if it <em>crashes</em>. A clean, deliberate exit looked like success, so nothing restarted it, and the node sat dead showing "server starting" with nothing able to start it. One of ours was down over two hours this way, and three of four checked had the same setting waiting to bite. Your node now repairs that setting itself the first time it starts.</p>
                  <p><strong>Portainer opens again.</strong> Its screen reported the app as not responding because the app was quietly refusing to start: nodes have been running Portainer 2.39.1 and their stored data was written by that version, while the app list pinned a version from two years earlier — so a rebuilt container landed on the old one, which will not read newer data. The correct version is pinned now, older installs upgrade cleanly, and no data was touched.</p>
                  <p><strong>Bitcoin starts reliably again.</strong> A leftover settings file in the Bitcoin folder — one the node kept rewriting and Bitcoin no longer reads — is treated as fatal by Bitcoin, so affected nodes restarted every few seconds forever. The node no longer writes it, clears stale copies, and treats any that remain as harmless.</p>
                  <p><strong>Every app screen opens from My Apps again.</strong> The login gate refused to be displayed inside another page at all — which is exactly how My Apps opens an app — so protected apps looked broken. It now allows only your own node to display it and refuses everyone else.</p>
                  <p><strong>The app login screen looks like the node's own now:</strong> same rotating artwork, same panel, the Archipelago mark, and the app's real icon as a tile the way My Apps shows it, instead of a plain box with a letter.</p>
                  <p><strong>The Mesh screen uses wide displays properly.</strong> On very large screens it stacked all five panels, clipping three headings to a sliver and squeezing the map into a letterbox. It now shows one panel at a time, filling the space, with the map edge to edge.</p>
                  <p><strong>You can choose how long you stay signed in.</strong> Settings → Account offers an inactivity timeout, a hard limit, and an option to re-enter your password before sending funds. TV and kiosk screens are never signed out for sitting idle — there is nobody there to sign them back in.</p>
                  <p>Updates now come from source.archipelago-foundation.org rather than a bare address, with the old one kept as an automatic fallback. Also: clearer wallet errors from ecash mints, and mesh peers reconnecting via their last known address before falling back to the wider network.</p>
                  <p>Known gaps, disclosed rather than buried: three voice-assistant ports remain open without authentication. Non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. The 5x real-node lifecycle gate was not run for this release.</p>
                </div>
              </div>
              <!-- v1.7.123-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.123-alpha</span>
                  <span class="text-xs text-white/40">August 5, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Five more screens on your node were readable by anyone who could reach it — and the last release's own check said they were fine.</strong> The Bitcoin, Lightning, Electrum, FIPS mesh and Fedimint Guardian screens each answered on their port with no login. They were missed because they run directly on the node's network rather than behind its container plumbing, so there was no address to pin and their descriptions listed no port at all — and the node builds its list of what to protect from exactly those descriptions. It neither protected them nor listed them as unprotected. Found by scanning the node from another machine instead of asking the node about itself.</p>
                  <p><strong>What was readable was the page, not your money.</strong> Every request on those ports that could have returned a credential — the Lightning connection details, the wallet passthrough, container logs, and every node command — already required a login and still refused without one. The Lightning macaroon fix from v1.7.120 was checked directly rather than assumed. What leaked was the screen itself: layout and code, no wallet data, no keys.</p>
                  <p>All five now serve only to the node itself, with the login gate in front, exactly like the twenty app screens closed in the last release.</p>
                  <p><strong>Every port on your node now has a stated policy — none are left undecided.</strong> Eleven had no instruction either way and stayed open by default. The BotFights arena, the router screen and the Pine voice screen now ask for your node password. The ones that genuinely cannot take a login page stay open with a written reason: Fedimint's guardian and gateway connections, NetBird's management and dashboard ports (your VPN devices carry their own credentials and cannot hold a browser session), Pine's secure listener, and the Lightning REST port that wallets reach with a macaroon exactly as before.</p>
                  <p>Freshly flashed nodes are covered too, not only existing ones — those five screens ship as prebuilt images, so a new node would otherwise have come up open even after this fix. All five were rebuilt, published, then pulled back and inspected to confirm the fix is really inside them.</p>
                  <p>Known gaps, disclosed rather than buried: non-browser clients — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — meet the login page and need an access token. Three voice-assistant ports remain open without authentication; the correct fix puts them on a private network with the assistant. The 5x real-node lifecycle gate was not run for this release.</p>
                </div>
              </div>
              <!-- v1.7.122-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.122-alpha</span>
                  <span class="text-xs text-white/40">August 4, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Your apps now ask for your node password before they open — over your home network, Tailscale, the mesh and Tor alike.</strong> Until now anyone who could reach your node could open Immich, Nextcloud, Vaultwarden, Jellyfin, Grafana and the rest simply by typing the address and port, with no login at all. Twenty app screens now sit behind the same login you use for the node, showing you which app you are opening, and honouring two-factor if you have it switched on. Logging in at an app address logs you into the dashboard too, so it is one password, not one per app.</p>
                  <p><strong>The things that must stay open stayed open.</strong> Zeus and other remote wallets still reach your Lightning node directly, Electrum wallets still connect, and Bitcoin still talks to its peers — those connections carry their own proof of identity and a login page would simply break them. Every one of these exceptions now has to state in writing why it is safe to leave open, so the list is something you can read rather than something you have to discover.</p>
                  <p><strong>A private address on your node was answering the mesh without a password.</strong> One app's port was marked as being for this machine only, and the part of the node that carries mesh traffic did not know that — it forwarded requests from the whole mesh straight to it. Found while checking the work above on a real node, not in testing. That path now refuses anything marked machine-only, and the app is reachable only from the node itself, as intended.</p>
                  <p><strong>Tor addresses no longer skip the login.</strong> An app published as a .onion address was handed straight to the app, because a Tor visitor carries no session. Those addresses now arrive at the login gate first, closing the last route that went around it.</p>
                  <p>Nodes fix themselves after this update. Apps installed before this system used its current container setup kept their old wide-open address even after being told to move, and each would have needed hand-holding on every node. Your node now notices the difference and rebuilds those apps itself, keeping their data, within about half a minute of starting.</p>
                  <p>The node had been reading two different sets of instructions about its own apps — the signed list it downloads, and older copies on disk — which is how a port meant to stay private was briefly opened. Both now come from the signed list.</p>
                  <p><strong>The key that signs these updates has been replaced.</strong> The previous signing key was exposed where it should not have been, so it is treated as compromised and this release installs its replacement. This update is the last one signed with the old key, by necessity — it is the one that teaches your node the new one.</p>
                  <p>Known gaps, disclosed rather than buried: eleven app ports still have no stated policy — BotFights, the Fedimint gateway, NetBird, the voice assistant's own screens and the router screen — and remain reachable without a login until each is decided deliberately; the node reports them rather than guessing, because guessing at an unstated setting is what caused two incidents this week. Three voice-assistant ports are still open without authentication; the correct fix puts them on a private network with the assistant. Non-browser apps — phone apps for Vaultwarden, Home Assistant or Jellyfin, and git over the web — will meet the login page and need an access token; say the word if you use one and it can be exempted. The 5x real-node lifecycle gate was not run for this release.</p>
                </div>
              </div>
              <!-- v1.7.121-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.121-alpha</span>
                  <span class="text-xs text-white/40">August 4, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Making another node "Trusted" now asks for your node password.** Trust was being handed out by machines rather than by you: any node able to reach yours could join and mark itself Trusted, because the check proved only that the caller owned the key it had just presented — never that you had approved it. Trust also spread on its own, since every peer a Trusted node advertised was added as Trusted too, so one grant quietly propagated across the whole federation. Uninvited joins are now capped at Observer, advertised peers arrive as Observers, and raising anyone to Trusted — whether by generating an invite or by changing the dropdown on a node — requires your password. Lowering trust deliberately does not, because the safe action must never be the inconvenient one. Existing peers are left exactly as they are rather than silently demoted, and each one now records how its trust was granted so you can review them.</p>
                  <p>**Nodes you have peered with can be messaged straight away.** Peering was not enough: you also had to be within LoRa radio range of the other node once before chat would work. The node picked how to send a message based on which radio was plugged in, and only one of those paths knew how to reach a peer over the mesh's internet transports — so on a node with a different radio, or no radio at all, messaging a peer you had just federated with simply failed until a radio contact happened to appear. Peered nodes are reachable without radio by definition, so that choice no longer depends on the hardware. Radio is still preferred when the other node is actually in range and the message fits.</p>
                  <p>The dashboard no longer flickers a vertical line across its cards. A rendering seam appeared at random while moving the mouse, because the two large cards used a background-blur effect that this system already disables everywhere else on the dashboard — that browser mis-draws it inside the dashboard's animated container, and these two cards had been missed when the workaround was written. Diagnosed from a single screenshot rather than by trying to reproduce it.</p>
                  <p>The Lightning screen will actually update from now on. Its image was set to "latest", and the container system will not re-fetch a label it already holds, so nodes kept the same Lightning screen forever no matter how many updates shipped. A separate copy of the same setting used only by brand-new installs also described the screen incorrectly, so fresh installs got a screen that never answered.</p>
                  <p>Apps that provide their own screens stop rebuilding themselves in a loop. On this system's own node one of them rebuilt every thirty-five seconds indefinitely, burning processor time and restarting the app each round. The node decided a rebuild was needed by comparing file dates against the image's creation date, but a rebuild that changes nothing reuses the existing image and leaves that date untouched — so the condition that triggered the rebuild was still true afterwards, forever. Nodes taking this update repair themselves the first time they check.</p>
                  <p>Groundwork you can see but that does not change access yet: the node can now tell you which of its app ports answer without a login, and every port that is deliberately open — Bitcoin's peer connections for syncing the chain, Lightning's wallet connections, the Electrum wallet protocol — now has to state in writing why it is safe, so the list of exceptions is something you can read rather than something you have to discover. The login gate that will sit in front of the rest is built and proven working end to end on a real node, but it is not yet closing any ports; that arrives with the signed app catalog that tells each app to hand its address over.</p>
                  <p>Releases can no longer ship an unsigned update file. Signing was skippable, and when it was skipped the release was still committed and tagged — producing an update that every node correctly refuses to install. It had been caught by hand every cycle; now the release simply stops.</p>
                  <p>Known gaps, disclosed rather than buried: the 5x real-node lifecycle gate was not run for this release. App ports other than the deliberate exceptions above are still reachable without a login — the gate reports them, and closing them needs the next signed catalog. Three voice-assistant ports are open without authentication and should not be; the correct fix puts them on a private network with the assistant instead, which needs testing on a node that runs both. Two nodes on the fleet still share SSH host keys (detection shipped, rotation remains a deliberate operator decision).</p>
                </div>
              </div>
              <!-- v1.7.120-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.120-alpha</span>
                  <span class="text-xs text-white/40">August 2, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>**Security, and the reason to take this update: two ports on your node handed anyone who could reach them complete control of your money, with no password.** The Lightning app's port answered a plain web request with the LND admin macaroon, the TLS certificate and the node's onion address — everything needed to drain the wallet remotely, and the onion meant an attacker kept that ability even after losing access to your network. The Bitcoin app's port reached Bitcoin Core's control interface using credentials the node itself supplied on the caller's behalf, with a wallet loaded. Anything on your home network, your Tailscale network or the mesh could use either one. Both now require you to be logged in. If your node has been reachable by anyone you do not fully trust, treat the Lightning macaroon and the Bitcoin RPC password as known to them.</p>
                  <p>The Bitcoin and Lightning app screens can no longer be published as public Tor addresses automatically. They were one app-id away from being handed a worldwide, permanent address as a silent side effect of being installed — which would have re-opened the hole above to the entire internet. Turning Tor on for them deliberately still works; it just never happens on its own.</p>
                  <p>**Fixes shipped inside the program now actually reach apps that your system keeps running.** A container the node had been told to uninstall, but that the system service manager kept alive anyway, was quietly skipped by the part of the node that applies configuration — so it never received updates that shipped with the program. This was found the hard way: the Bitcoin control-interface fix above appeared to be installed and silently was not, while the Lightning half applied correctly, which is the most misleading way for a security fix to fail. Both halves are now proven to land on a real node.</p>
                  <p>The Lightning and Bitcoin node screens have been rebuilt to match what umbrelOS offers. Lightning gains Overview, Channels, Activity, Insights, Connect and Settings tabs with a sats/BTC switch; Bitcoin gains Insights, Peers, Connect and Sharing. Along the way: every copy button on those screens silently did nothing (the browser blocks clipboard access inside an embedded page) and now works; the channels link led to a dead page; and Node ID showed a bare key instead of the full address someone can actually connect to.</p>
                  <p>Updates to the Bitcoin screen show up without a hard refresh. The page was being cached by the browser, so a freshly updated screen kept rendering the previous one.</p>
                  <p>The AI sidebar loads again. It was asking for its program files at an address that pointed at the main app's files, where they do not exist, so it silently loaded nothing.</p>
                  <p>The navigation above the bottom bar no longer follows you between screens. Back buttons and the mesh tab bar stayed pinned over every other page once you had visited the screen that owns them. Keeping tabs loaded in the background — the change that made switching between them instant — means leaving a screen hides it rather than destroying it, and this floating navigation sits outside the screen it belongs to, so it was never being hidden with it. It is now tied to whether its own screen is on display. The speed is unchanged: the screens are still kept loaded, so returning to one is still instant.</p>
                  <p>Wallet: Lightning actions are now offered based on whether you actually have a usable channel rather than just a running node, sending is gated the same way, and an invoice you cannot yet receive offers to install a Lightning node instead of simply failing.</p>
                  <p>Onboarding and viewing fixes: the "I have written down my recovery words" tickbox is findable on short screens, paid pictures and videos open in the app's own viewer with a visible loading state instead of a blank browser tab, picture-in-picture survives changing tabs, and the FIPS/Tor labels on peer cards stay put instead of wrapping into the card below.</p>
                  <p>Key-material hardening across the node: a node that is already set up refuses to have its identity replaced by an unauthenticated request; first-boot secret generation now fails loudly instead of silently continuing with shared keys; the node proves its TLS certificate and key are actually a matching pair; the Bitcoin Core wallet path that kept a second copy of your spending key outside the encrypted store has been removed; and every place the node generates a key, token or nonce now names its source of randomness explicitly, enforced at build time.</p>
                  <p>Federation and mesh: a rotated gateway credential now reaches the already-running container instead of leaving the old one in place, sync failures are surfaced to you instead of being swallowed, and nodes can share their Lightning connection details with a chosen peer over the mesh — the groundwork for opening channels with nodes you already talk to.</p>
                  <p>Known gaps, disclosed rather than buried: the 5x real-node lifecycle gate was not run for this release. Two nodes on the fleet still share SSH host keys with each other (detection shipped, rotation is a deliberate operator decision and has not been performed). Bitcoin Core can now reach Tor from its container, but is not yet routed through it — the network mode is becoming a setting you choose, and until then Core's peers remain on the clear internet.</p>
                </div>
              </div>
              <!-- v1.7.119-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.119-alpha</span>
                  <span class="text-xs text-white/40">July 31, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Wallet payments now work on nodes whose channels are private/unannounced. Every invoice-creation call site — the wallet's own Receive flow, and the seller-side paid-content/peer-files flow — only ever sent LND the amount and memo, so LND defaulted private to false and returned invoices with no route hints. Any node whose only usable channel is private or unannounced (the common shape for a channel someone opened to you) was silently unpayable through the wallet, and unpayable through paid file/content sales too. Both call sites now set LND's private flag correctly; this was broken in the field and is the main reason for this release.</p>
                  <p>Tor and the mesh's Tor fallback are reliable again. The node's background "doctor" health-checker was fighting Tor over the permission bits on its own hidden-service directory: it compared the directory's mode against the literal string "700", but Tor's own setgid hidden-service mode is 2700 — a value the doctor's check never recognized as correct. Every ~5 minutes it "corrected" the mode back to 700 and restarted Tor to apply it, and Tor immediately reasserted 2700 — a permanent restart loop that meant Tor could never hold onto its consensus/HSDir cache long enough to be useful, breaking the mesh's Tor fallback path entirely. The check now compares only the owner/group/other bits that actually matter (both 700 and 2700 pass; genuinely wrong modes like 750 or 2755 are still corrected and restart Tor), plus a 30-minute restart backoff so no future condition can reproduce the storm.</p>
                  <p>Wallet balances and your node's own FIPS identity key (npub) are no longer written to the browser's sessionStorage — caught by an audit of the page-caching work below. Every cache call site in the app now makes an explicit, reviewed decision about whether its data is allowed to persist across a reload, and a one-time migration purges any legacy, unaudited snapshot left behind by an older build.</p>
                  <p>Server, Home, Mesh, Chat/AI chat, and the secondary screens (app details, marketplace, cloud, federation, monitoring, router/OpenWrt) now load instantly from cache when you revisit them and refresh quietly in the background, instead of blanking and re-fetching everything on every tab switch — this closes out the page-performance work started back in v1.7.116/117.</p>
                  <p>App updates (including this one) now apply automatically in the background instead of waiting on a tap-to-update prompt, matching how kiosk/TV installs already behaved — the reload still waits for any in-progress splash/dashboard animation to finish first, so it won't land mid-motion. This was a direct, explicit decision made with the mid-payment-reload risk spelled out in advance; reverting to a confirmation prompt for beta is a one-line change if wanted later.</p>
                  <p>Known gap, disclosed rather than buried: the project's 5x production lifecycle gate (install/UI/stop/start/restart/reinstall/reboot-survive/archipelago-restart-survive/uninstall, run on a real node — CLAUDE.md's own definition of done before a release tag) was NOT run for this release, because its target node was unreachable and running it here would have required rebooting a shared, live build machine out from under other active work. This release's own automated gates (release-gate harness, strict catalog-drift check, the full cargo test suite, a mount-level ISO smoke test, and a headless QEMU boot test) all still ran and passed — this is specifically about the separate 5x real-node lifecycle gate, which is still outstanding and should be run as soon as the node is reachable again.</p>
                </div>
              </div>
              <!-- v1.7.118-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.118-alpha</span>
                  <span class="text-xs text-white/40">July 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fixes mesh radios dropping off on nodes that took the v1.7.117 update. Updates only ever replaced the main program, never the packaged radio helpers — so updated nodes were left running an older radio daemon that didn't understand a new option and quietly gave up, showing "device not connected" with a Connect button that did nothing. The node now checks what its radio daemon supports before using new options, and updates finally carry the radio helpers themselves, so every node gets current radio support with the update instead of only from a fresh install.</p>
                  <p>The in-app "Flash LoRa" flow works on updated nodes. The RNode flashing tool was only ever included on freshly installed nodes; everywhere else flashing failed with a cryptic "No such file or directory". The tool now ships with updates and is included on new install images, and if it's somehow still missing the error says exactly what to do instead.</p>
                  <p>Message notification badges finally remember what you've read. Unread counts were only kept in memory, so every visit re-counted old messages as new — including a phantom badge for chats with nothing new in them. Read-state is now saved on the device, opening a chat marks all its linked conversations read, and history no longer re-badges after a reload.</p>
                  <p>Every mesh message now has a visible "⋯" button that opens the route view: watch the path your message took animate — sender and receiver appear, a pulse travels the link, and each relay hop lights up in order — with signal quality for radio links and delivery status. (Tapping the transport pill still works too.)</p>
                </div>
              </div>
              <!-- v1.7.117-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.117-alpha</span>
                  <span class="text-xs text-white/40">July 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Flash your LoRa radio from inside the app. The Mesh page now has a "Flash LoRa" button that opens a guided flow: pick the firmware family (MeshCore, Meshtastic, or Reticulum RNode) and your board, and the node downloads the latest release and flashes it with live progress — no external flasher website, no cables to a computer. The same flow appears when a freshly plugged-in radio is detected, and a long list of flashing pitfalls was fixed along the way: radios no longer boot-loop after a flash, failures show the real error instead of silently bouncing back, wedged flash jobs can't get stuck forever, and board auto-detection no longer misidentifies Heltec boards.</p>
                  <p>Every Archipelago node now acts as a Reticulum relay. Nodes forward mesh traffic and re-broadcast peer announcements, so two radios that can't hear each other directly can still discover and message each other through any Archipelago node in between — your nodes become infrastructure for the whole neighbourhood mesh, including non-Archipelago apps like Sideband.</p>
                  <p>Reticulum (RNode) radios are now first-class mesh citizens. Radios are reliably detected on node startup (a boot-timing race used to leave them unclaimed), settings changes apply live without a restart, your node's name propagates over the Reticulum network so other apps like Sideband see it properly, and a crashed Reticulum daemon is detected and restarted automatically. Photo and file attachments sent over Reticulum now actually arrive — four separate delivery bugs were found and fixed, verified end-to-end over real radio hardware.</p>
                  <p>Messages to contacts that exist on both the internet mesh and a LoRa radio now prefer the radio when it's live, and attachments follow the same path — so co-located nodes talk over the air even when the internet path exists.</p>
                  <p>Mesh chat polish: each message in the image viewer shows which transport carried it, a new hop-route view shows the path a message took, reactions moved into a tidy dropdown, and read-tracking now reflects what you've actually seen. The Refresh and Broadcast buttons give real feedback, and the radio-setup modal shows honest probe progress instead of freezing.</p>
                  <p>The wallet transactions list works properly on phones now: it scrolls (it silently couldn't on touch screens before), and the All / On-chain / Lightning / Ecash filter tabs stay pinned at the top with a subtle blur while the list scrolls underneath.</p>
                  <p>Backend services no longer masquerade as launchable apps. Anything without a real web interface — databases, APIs, background workers, including stacks you deploy by hand for testing — now files under Services with no Launch button. Apps declare their interface in their manifest; for everything else the node checks the port itself to see whether a browser page actually lives there.</p>
                  <p>Lightning payments that take a while (slow multi-hop routes) are no longer reported as failed while they're still in flight. The wallet now waits properly, shows an honest "pending" state, and reports the true final outcome.</p>
                  <p>Server pages feel instant: Server, Federation, Lightning channels, Monitoring, wallet, Cloud, and Credentials screens now render immediately from a shared cache and refresh live in the background (including push updates over the node's websocket), instead of blanking while every panel refetches.</p>
                  <p>The node stays responsive under heavy load: the connection handler now sheds excess load instead of stalling everything behind it, and companion-app probes no longer trigger container builds during routine checks.</p>
                  <p>FIPS mesh uptime hardening continues: the node's peer port is opened explicitly everywhere, LAN anchors use the right port, direct peering between co-located nodes works again, dials fail fast instead of hanging, and a connectivity watcher re-applies anchors immediately when the network comes back.</p>
                  <p>FIPS startup is more reliable on nodes that have the packaged fips.service instead of Archipelago's archipelago-fips.service. Startup self-heal, onboarding, dashboard Start, and reconnect now use the systemd unit the node actually has, so FIPS no longer looks like it needs to be installed when it only needs to be started.</p>
                  <p>App screens over the FIPS mesh now bind their relay only to the node's FIPS address instead of reserving the same host ports Podman needs. This keeps apps such as FileBrowser and Botfights from restart-looping because the backend was already holding their published ports.</p>
                  <p>Companion app 0.5.25: a redesigned settings hub (three-finger tap opens it over the dashboard), seamless transport handoff with FIPS mesh settings, the wallet scanner reads dense invoice QR codes, app webviews clear the phone status bar with an HTTPS toggle on add/edit, and off-LAN loads fall back to the mesh URL instead of a dead LAN address.</p>
                  <p>Public-source preparation now includes a Nostr Git hosting plan using ngit, NIP-34, and GRASP: anyone can clone, fork, review, and propose changes from their Archipelago node, while canonical merge authority stays with a small signed maintainer set in the style of Bitcoin Core.</p>
                </div>
              </div>
              <!-- v1.7.116-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.116-alpha</span>
                  <span class="text-xs text-white/40">July 27, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nodes no longer get stuck on "server starting up" after an update or reboot. The backend now reports ready immediately and recovers its apps in the background, and it always restarts itself if it ever goes down — the days-long "server starting up" hang is gone.</p>
                  <p>Installing apps no longer crashes the node. A recent change that made app screens reachable over the mesh was holding onto every app's port in advance, so installing an app collided with it and the port-cleanup took the backend down and rolled the install back. Installs are clean now.</p>
                  <p>Rolls up v1.7.115: app screens and the dashboard load over the mesh out of the box, with IPv6 support end to end, and nodes rejoin the mesh in seconds after their rendezvous point restarts.</p>
                </div>
              </div>
              <!-- v1.7.115-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.115-alpha</span>
                  <span class="text-xs text-white/40">July 26, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The companion app can reach your node's screen from anywhere again. The recent security hardening locked down the node's mesh interface so tightly that the dashboard itself was blocked — the phone would pair and connect, then sit on a blank screen. The node now explicitly opens its own web interface (and only that) through the mesh firewall on every install and upgrade, so the phone's view of your node works out of the box, on any network, and can't silently break in a future update.</p>
                  <p>The node's web interface also answers on IPv6 everywhere it answers on IPv4 — the mesh runs entirely on IPv6, and one v4-only listener was enough to make a working connection show nothing.</p>
                  <p>Nodes now come back onto the mesh in seconds instead of minutes after their rendezvous anchor restarts: the fast-reconnect tuning proven on the phone this week is now baked into every node's mesh configuration, and it survives upgrades.</p>
                </div>
              </div>
              <!-- v1.7.114-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.114-alpha</span>
                  <span class="text-xs text-white/40">July 26, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Plugging in a mesh radio no longer traps it in an endless reboot loop. The device detector itself was causing it: every scan pulsed the radio's reset line, the same board was probed twice under two names, and retries came so fast the radio never finished booting before the next reset hit. Detection now gives the board real time to boot, probes it once, backs off properly between attempts, and no longer fights the "device detected" popup for the port. Radios that could never connect now come up within a minute of being plugged in.</p>
                  <p>The Lightning channels screen now has All / Active / Pending / Closed tabs. Pending gathers everything in motion (opening, closing, force-closing — each with its own status dot and a link to the closing transaction), and Closed is a real history: how each channel ended, what settled back to you, and the closing transaction for each.</p>
                  <p>Sending bitcoin on-chain now puts you in charge of the network fee: pick Fast, Standard, or Slow (Standard is the default), or set your own target blocks or sats-per-vByte. The confirmation step shows the estimated fee for your chosen speed before any money moves.</p>
                  <p>Type on-chain amounts in whichever unit you think in — a sats/BTC switch on the amount field converts as you type.</p>
                  <p>Back up your seed by scanning it. Every recovery-phrase screen (onboarding, Settings, and the Lightning wallet seed) now has Words and QR code tabs — words always shown first. The QR for your node's recovery phrase uses the SeedQR standard, so hardware wallets like Passport Prime, SeedSigner, and Keystone can import it with a single scan (a plain-text option remains for wallets that read the phrase as text). The Lightning seed's QR is plain text with an honest note: it's an LND-format seed that restores into Lightning wallets like Zeus or Blixt, not into hardware wallets.</p>
                </div>
              </div>
              <!-- v1.7.113-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.113-alpha</span>
                  <span class="text-xs text-white/40">July 25, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fixed a money bug in Cashu ecash sends: the token you handed a recipient could carry your own change proofs along with it, letting the same sats be credited twice. Change now stays in your wallet — only the amount you meant to send leaves it.</p>
                  <p>Closing a Lightning channel is no longer a leap of faith. The close used to hang (or time out with an error) even though it had actually gone through; it now comes back within seconds with the closing transaction ID. Channels mid-close appear in the channel list as Closing or Force-closing with their transaction attached, and a new closed-channels history keeps past closes visible instead of letting them vanish from the list.</p>
                  <p>The wallet card now leads with your total bitcoin across everything, and the on-chain balance gets its own chain icon so the rows read at a glance.</p>
                  <p>The companion phone app (0.5.15) connects dramatically faster away from home: a cold connect over 5G dropped from 40+ seconds to about 5. First connects no longer stall on unreachable mesh dial hints, fresh joins fail fast and retry instead of waiting out long timeouts, and the phone re-announces itself the moment the network around it changes. The node side's mesh-join handling was hardened to match.</p>
                </div>
              </div>
              <!-- v1.7.112-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.112-alpha</span>
                  <span class="text-xs text-white/40">July 23, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Sound works on TVs out of the box. Fresh installs were missing the audio system entirely, and even when present a boot-time race left HDMI silent until the cable was unplugged and replugged. Both are fixed: installer images now ship the full audio stack, and a small background helper detects the silent-HDMI state and heals it automatically.</p>
                  <p>Plug in a game controller and drive the whole TV interface with it — navigation, menus, and media playback all respond to the gamepad, and dialogs that pop up are controller-navigable too.</p>
                  <p>The companion phone app took a huge leap (0.5.9). Your node and its apps now work from anywhere — on 5G or any internet connection, the phone reaches the node over the encrypted mesh with zero port forwarding or VPN setup. Startup away from home is instant, apps on your node open inside the app, the phone's native camera handles QR scanning, and a branded full-screen loader shows while the mesh connects.</p>
                  <p>Mesh Party: two phones scan each other's QR and instantly get a direct encrypted chat and app sharing between them — plus a "Share this app" QR that anyone can scan with a normal camera to install the companion app.</p>
                  <p>Pairing a second phone no longer silently logs out the first. Every device now keeps its own named access credential, ending the mystery reconnects when a household paired more than one phone.</p>
                  <p>The companion pairing QR is scannable again (it had grown too dense for phone cameras) and now identifies your node by its identity key, so the app recognizes your node even after it moves or gets a new address.</p>
                  <p>Every app your node serves on your home network is now also reachable over the mesh — remote access covers the apps themselves, not just the dashboard.</p>
                  <p>Selling files: you now choose which payment methods you accept (Lightning, ecash, …) and buyers are only offered those — enforced by the node itself, not just the buttons. Paying twice for the same file is impossible now, purchases file themselves into a new Paid Files tab, purchased music always plays in the bottom-bar player, and videos get picture-in-picture.</p>
                  <p>Sending Lightning is invoice-first: paste or scan an invoice and the amount fills in and locks by itself. An expired invoice now tells you plainly to ask for a fresh one instead of failing cryptically, and payment errors always reach your screen.</p>
                  <p>Sending to a pasted address gets a confirmation step showing exactly what will happen before any money moves, and buying ecash is an explicit two-step — no more accidental purchases.</p>
                  <p>Apps keep running when you change how they're displayed. Switching an app between windowed and fullscreen used to reload it from scratch (stopping any playing media); the app now stays live through the switch, and each app remembers its own preferred display mode.</p>
                  <p>A watchdog notices when the Lightning (LND) node wedges and revives it before you do; Fedi ecash gets its own send option with scannable token QR codes.</p>
                  <p>Fedimint's Lightning gateway and guardian now follow whichever bitcoin version is actually running instead of pointing at a stale address — switching bitcoin versions no longer strands them.</p>
                  <p>If your router starts handing out different addresses, the Pine voice speaker re-links itself automatically instead of staying silent until someone re-configures it.</p>
                  <p>Polish: mesh radios never show garbled device names anymore, the TV kiosk uses slim overlay scrollbars instead of fat grey bars, and the AI chat's background artwork shows through again.</p>
                  <p>Your mesh messages now survive restarts. Chat history — channels and DMs alike — used to live only in memory, so a reboot or update wiped every conversation; worse, other nodes silently discarded the first messages you sent after a reboot. Everything is now saved on the node and restored on startup, and post-reboot messages deliver reliably.</p>
                  <p>Plug in any LoRa radio and the node walks you through it. A setup window appears every time a radio is connected, shows what firmware is already on it (MeshCore, Meshtastic, or Reticulum RNode — with its current name, region, and channels where available), and offers two honest choices: "Set Up with Archipelago Settings" (a preview screen shows exactly what will be written before anything touches the radio) or "Keep As Is" (the radio is used untouched, and you can hot-swap radios freely). Swapping sticks mid-session now just works — including Reticulum RNodes, which fresh installer images now support out of the box.</p>
                  <p>Incoming bitcoin appears in your wallet within seconds of being sent — balance and the yellow "unconfirmed" entry update live, no refresh, no waiting for the next poll.</p>
                  <p>The speaker now announces the very first mesh message a node ever receives, and DMs announce just like channel messages (a safety guard against announcement storms was quietly swallowing them). Announcements also react about twice as fast.</p>
                  <p>Opening a Lightning channel right after the node starts no longer fails with a scary red error. The node quietly retries while Lightning finishes waking up, and if it's still not ready you get a calm "still finishing its startup — try again shortly" notice instead.</p>
                  <p>Viewing a transaction works on every node now, including small ones. Nodes with pruned bitcoin storage can't run the Mempool explorer app; transaction links now open your choice of external explorer instead (tx1138.com by default) — after a clear one-time warning that a third-party server will see which transaction you looked up. Set your preferred explorer in Wallet Settings → the new On-chain tab.</p>
                  <p>Voice commands respond noticeably faster: speech recognition now transcribes in roughly half the time, with identical accuracy on short commands.</p>
                  <p>Scanning a Lightning invoice with your phone's camera is far more reliable — dense invoice QR codes that the photo scanner missed are now read by the phone's native barcode engine.</p>
                  <p>The companion app's pairing QR always contains an address your phone can actually reach. If you manage your node over a VPN (Tailscale), the QR used to embed the VPN address, and pairing silently failed; it now advertises the node's home-network address.</p>
                  <p>Peer requests sent from Nostr discovery now actually arrive: your node checks for incoming requests every five minutes by itself (previously they sat unseen until someone manually pressed "Poll"), requests publish to all your configured relays instead of two hardcoded ones, and a failed send tells you instead of pretending it worked.</p>
                  <p>The Connected Nodes list refreshes instantly. It previously froze for up to 30 seconds per offline peer while checking who's reachable, one peer at a time; the checks now run all at once in the background while the list shows immediately.</p>
                  <p>Apps opened from inside a window (like a transaction from the wallet) now animate smoothly on top instead of loading invisibly underneath.</p>
                  <p>Settings-style windows keep their tabs pinned at the top and their buttons pinned at the bottom; only the middle scrolls. The wallet's tabs are now Channels / Cashu / Fedi / Ark / On-chain so all five fit.</p>
                  <p>On the TV screen, menus no longer flash open and instantly close. And the interface never follows your computer's light/dark preference anymore — dropdowns and other native controls stay dark on every device.</p>
                  <p>Error messages tell you what's actually wrong: "Insufficient balance: need 80 sats, have 0 sats" now reaches your screen instead of "Operation failed. Check server logs."</p>
                  <p>Installing Mempool no longer refuses to start while ElectrumX is mid-resync (it connects by itself once ElectrumX is ready), and installs no longer fail just because the system was momentarily busy.</p>
                  <p>Much quieter logs: the node no longer tries to start containers that are already running (hundreds of harmless-but-alarming errors per day), and a node that's offline stops hammering unreachable servers every 30 seconds with rebuild attempts.</p>
                  <p>Phones pairing with the companion app connect over the node's embedded mesh for remote access, with instant QR pairing and per-device access tokens (contributed alongside this release).</p>
                </div>
              </div>
              <!-- v1.7.111-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.111-alpha</span>
                  <span class="text-xs text-white/40">July 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Ask your node anything, out loud. Install Pine (the voice assistant app) alongside Home Assistant and everything wires itself automatically: speech recognition, the speaking voice, and a Claude-powered brain. Questions about your node — "what's the block height?", "how many peers am I connected to?", "is bitcoin synced?", "what's my Lightning balance?" — are answered instantly from the node itself without costing anything; anything else goes to Claude for a real conversation. New mesh radio messages are read out on your speaker as they arrive.</p>
                  <p>Pine now ships a wake-word listener, so a paired speaker can sit on standby and activate when it hears its wake word instead of needing a button press. (A custom "Yo Archy" wake word is in the works.)</p>
                  <p>Pine's launcher page shows your node's live status at a glance: software version, uptime, bitcoin sync progress, and mesh peers.</p>
                  <p>Fixed: installing Pine could send Home Assistant into a crash loop on startup (a record the installer wrote was missing a timestamp field Home Assistant requires). Two noisy warnings that repeated in Home Assistant's log every half minute are silenced too.</p>
                  <p>The companion phone app opens every app in its fast built-in browser view again, with native back/forward/reload controls, instead of embedding some apps inside the page where they scroll and render worse. This had quietly regressed.</p>
                  <p>Turning on federation discovery now shows you exactly what you're about to sign: a panel explains the announcement before your key signs it, you can review the signing details any time from the discoverability strip, and the panel fits and scrolls properly on small phones.</p>
                  <p>Fixed a bug on nodes using the newer app-management engine where Bitcoin's access credentials were written out incorrectly (a placeholder leaked through as the literal text "/bin/bash"), which broke the node's Bitcoin status display, Lightning's connection to the chain, and any app that reads Bitcoin data.</p>
                  <p>Bitcoin's access credentials also moved out of the process command line into a protected file, so they're no longer visible to other software on the node.</p>
                  <p>Desktop app windows have one-click buttons to switch between side panel, overlay, and fullscreen viewing.</p>
                  <p>On the phone home screen, the wallet card moved up to sit right under My Apps.</p>
                  <p>Home Assistant updated to 2026.7.3, which keeps voice satellites (like Pine's speaker) connected reliably.</p>
                </div>
              </div>
              <!-- v1.7.110-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.110-alpha</span>
                  <span class="text-xs text-white/40">July 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Pay by pointing your camera: the wallet has a new Scan button (on the wallet card and inside both the Send and Receive windows) that reads any payment QR code — Lightning invoices, Bitcoin addresses, Cashu tokens, and Fedimint invites — and takes you straight to the right send or redeem screen with everything filled in. It also understands the animated, multi-part QR codes some wallets show for long payloads. If your browser can't open a live camera preview (common when reaching the node over plain http), a "Take photo of QR" button snaps a picture with your phone's camera and reads the code from the photo instead.</p>
                  <p>The TV screen got a complete overhaul. A deep bug made the display freeze on the intro artwork on 4K TVs — that's fixed, and along the way: the interface now picks a comfortable, sharp size for big screens (a 4K TV gets a full desktop layout at double sharpness), the artwork behind every page shows again instead of a black void, switching between tabs animates smoothly, the built-in AI assistant stays in its dark theme, and the Cashu and Ark wallet icons no longer render as empty squares.</p>
                  <p>You can now choose how big the interface renders on your node's attached screen: Settings → Display offers Auto (recommended), Large UI, Balanced, and Native — changing it applies immediately.</p>
                  <p>The companion phone app can steer the TV again. Remote input from the phone was being silently ignored on kiosk displays; the remote-control relay now runs there like everywhere else.</p>
                  <p>The companion app is also ready to grant its built-in browser camera access, so the wallet scanner can work inside the app (ships with the next companion app build).</p>
                  <p>Zero-amount Lightning invoices can now be paid: the wallet asks you for the amount and sends it along, instead of failing on invoices that leave the amount up to the payer.</p>
                  <p>The Lightning setup guidance now reads the same everywhere: "Open a channel with Zeus Olympus node and start sending and receiving Lightning payments. Minimum 150,000 · maximum 1,500,000 on-chain sats required."</p>
                  <p>Installer images now bundle a color-emoji font, so emoji anywhere in the interface render properly on the TV screen.</p>
                </div>
              </div>
              <!-- v1.7.109-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.109-alpha</span>
                  <span class="text-xs text-white/40">July 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Meet Pine, your node's voice assistant: a new app in the App Store that gives your node ears and a voice — speech-to-text and text-to-speech engines that run entirely on your own hardware, ready to wire into Home Assistant for private, offline voice control. Install it like any other app; nothing you say leaves your node.</p>
                  <p>Your node can now program its MeshCore radio's RF settings — frequency, bandwidth, spreading factor, and coding rate — from Mesh → Device settings. Radios that were flashed with mismatched settings could hear that other radios exist but never decode their messages, and until now the only fix was a separate phone app. Set the values once and the node programs the radio automatically (it restarts once to apply); every radio on your mesh must use the same values to talk to each other.</p>
                  <p>The Device settings panel is tidier: values you can edit (name, region, channel) are no longer also shown as separate read-only rows.</p>
                </div>
              </div>
              <!-- v1.7.108-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.108-alpha</span>
                  <span class="text-xs text-white/40">July 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Your node connects to the private mesh far more reliably. Nodes rely on a public rendezvous point to find each other, and the only one available was unreachable from many home and office networks — leaving some nodes unable to join the mesh at all. There is now a second, always-reachable rendezvous point, and your node tries every one it knows, so it joins the mesh in seconds instead of being stranded.</p>
                  <p>Wi-Fi setup now heals itself on older nodes. Some nodes set up before a mid-year fix couldn't connect to a Wi-Fi network from the screen — it failed with a permissions error — because the piece that lets the node manage networking on your behalf was missing. Nodes now put that piece in place automatically on startup, so "scan, pick a network, type the password, connect" works without reinstalling.</p>
                  <p>Your node rejoins the mesh within seconds after an update. Applying an update briefly restarts the mesh service, and previously a node could sit disconnected from other nodes for up to five minutes before it retried.</p>
                  <p>The TV screen now fits your television. On a large or 4K TV the interface rendered tiny with no way to zoom on a keyboard-less screen; it now sizes itself to a comfortable, readable scale automatically (and small laptop panels are left unchanged).</p>
                  <p>More TV-screen polish: the built-in assistant shows its dark theme instead of bright white panels, the on-screen hint for switching between the kiosk and a terminal now points at the right keys, the welcome logo no longer occasionally renders as garbled characters, and an accidental tap of the power button no longer shuts the node down — hold it to power off on purpose.</p>
                  <p>Behind the scenes: fixed the installer image build so it no longer stops on a component that was removed from the product, and so it correctly includes the private relay it was meant to bundle.</p>
                </div>
              </div>
              <!-- v1.7.106-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.106-alpha</span>
                  <span class="text-xs text-white/40">July 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nodes on the same network now find each other directly. Your node announces itself on your local network and connects straight to other Archipelago nodes nearby, instead of every connection having to be introduced by a public rendezvous server out on the internet. Peers in the same home or office stay connected to each other even when that server is unreachable, and they reach each other faster.</p>
                  <p>On a phone, the peer files screen tells you how you're connected again. The badge showing whether a peer's files are arriving over the fast mesh or over Tor was only visible on desktop — on narrow screens it disappeared entirely. It now appears next to the peer name on mobile too.</p>
                  <p>Your node's mesh settings can no longer be written in a way that breaks the mesh. The configuration file used to be assembled as free-form text, where one wrong setting would stop the mesh service from starting and quietly drop your node off the network. It's now generated from a checked description of the file, with tests that verify the exact output.</p>
                  <p>When your node has trouble reaching another node, the logs now record the real reason instead of a generic summary. A failure to open a peer's files previously logged only "Failed to connect to peer" and threw away the actual cause, which made these problems very hard to diagnose. Nothing changes on screen, and no internal detail is exposed.</p>
                  <p>Behind the scenes: the installer image now builds its mesh component at a fixed, known version instead of whatever upstream had published that day, so two images built from the same source are identical.</p>
                </div>
              </div>
              <!-- v1.7.105-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.105-alpha</span>
                  <span class="text-xs text-white/40">July 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fixed a failure loop where a node that lost power or was moved could get stuck on a blank "can't reach your node" screen forever: startup recovery no longer spends minutes retrying containers that no longer exist, and a genuinely large recovery is no longer cut off half-way and forced to start over. The node now reaches its login screen even after the messiest shutdown.</p>
                  <p>Phone tunnel setup (WireGuard) is now dependable: the QR screen automatically retries while a fresh install is still settling instead of dead-ending at "failed to fetch", and if your node has moved to a different network the QR and downloadable config now carry the node's current address instead of the old one.</p>
                  <p>Fixed the white screen some laptop displays showed right after the intro on v1.7.104.</p>
                  <p>The companion phone app no longer suggests installing the companion app from inside itself.</p>
                  <p>The Tor page now lists onion addresses only for apps you actually have installed — fresh installs no longer come with six pre-made addresses for apps that were never set up.</p>
                  <p>Running archipelago --version or --help on the command line now prints and exits instead of silently starting a second copy of the node, which could briefly disrupt running apps.</p>
                  <p>Behind the scenes: installer image builds now stop loudly if VPN components are missing instead of producing a broken image, and a background file-permission sweep runs far less often, reducing disk churn on busy nodes.</p>
                </div>
              </div>
              <!-- v1.7.104-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.104-alpha</span>
                  <span class="text-xs text-white/40">July 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Software updates are now much safer to receive: the node will never install an update that isn't completely downloaded and verified byte-for-byte, closing a rare bug where an interrupted or cancelled download could leave a node unable to start.</p>
                  <p>If a freshly installed update does fail to start, the node now notices and automatically restores the previous working version by itself — no manual rescue needed.</p>
                  <p>The Electrum server now works with whichever Bitcoin you run: it finds Bitcoin Knots or Bitcoin Core automatically instead of assuming Knots.</p>
                  <p>While the Electrum server is first building its index, its waiting screen now shows the ElectrumX app icon and live progress.</p>
                </div>
              </div>
              <!-- v1.7.103-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.103-alpha</span>
                  <span class="text-xs text-white/40">July 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Connecting from the phone app no longer replays the intro cinematic on a loop: signing in after scanning the pairing QR could accidentally trigger "Replay Intro" instead of logging you in. The companion app now lands you straight on your dashboard, and the Android app waits for the login screen to be ready before it types your password.</p>
                  <p>Pressing Enter in any password box now does what you expect — it signs you in or moves to the next field, and can no longer "click" a nearby button by mistake when using a controller or the companion app.</p>
                  <p>The public demo no longer interrupts you with an "Update Available" popup that reset the site back to the intro — demo visitors simply get the newest version on their next visit.</p>
                </div>
              </div>
              <!-- v1.7.102-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.102-alpha</span>
                  <span class="text-xs text-white/40">July 17, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The password you choose during setup is now truly your node's password: it also becomes the system login for console and SSH access, instead of leaving the factory default in place. If you ever renamed your node and the TV screen went black on the next boot, that's fixed too — renaming no longer breaks the kiosk display.</p>
                  <p>Setting up Lightning is now a guided journey: a fund-your-wallet step that shows a live countdown while Bitcoin syncs, suggested channels you can open straight into the Zeus mobile wallet with one tap, and a "finish setup" prompt that walks you to the end — goals now complete when you've actually done the steps, not just when apps happen to be running.</p>
                  <p>Pair your phone by pointing it at the screen: the companion app now connects by scanning a QR code — scan, and it fills in your node's address and logs you in. The App Store has a banner to grab the Android app, and the pairing flow can now also set up secure remote access so your phone reaches home from anywhere.</p>
                  <p>First installs are far more dependable: app downloads that stall now retry instead of hanging forever (the old "first install fails, the second works" pattern), big multi-part apps show their real download progress instead of sitting at "Preparing", Lightning no longer fails its first install over temporary hiccups, and a brand-new node now comes up with its core apps — file cloud and ecash wallet — even with no internet connection.</p>
                  <p>The installer image is about 160MB smaller and gets to a working screen faster, because the apps bundled for offline setup are now compressed.</p>
                  <p>The first-run experience keeps its magic: the typing intro is back on fresh installs and can no longer be cut short by a mid-play refresh — updates now politely wait for the cinematic to finish — and dark backgrounds stay dark instead of flashing black or white.</p>
                  <p>Your backups now include your secrets — including the key that protects your Lightning wallet's recovery seed — and there's a Download button to take a copy off the node; the seed-backup reminder now actually opens the backup flow when you tap it.</p>
                  <p>Networking Profits grew into a full dashboard, network cards keep their action buttons in reach on every screen size, "Connect to Mesh" goes to the right page instead of a dead end, and the identity pages got a round of mobile polish.</p>
                  <p>Behind the scenes: apps that report their own health are no longer second-guessed by a port probe (fewer false "restarting" states), and pressing arrow keys or a gamepad is once again the only thing that shows the controller focus ring.</p>
                </div>
              </div>
              <!-- v1.7.101-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.101-alpha</span>
                  <span class="text-xs text-white/40">July 15, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The wallet speaks Ark: a new Ark tab shows your Ark balance and history, you can send and receive over the Ark protocol, pay Lightning invoices from your Ark balance, and Ark payments appear in the transactions view with their own filter chip.</p>
                  <p>Every app you install now automatically gets its own private .onion address — your apps are reachable over Tor a few seconds after install, with no manual "Add Service" step.</p>
                  <p>"Add Service" in the Tor panel now works for every app, not just a fixed list — the node reads the app's actual web port, so apps like Gitea, Jellyfin, Nextcloud, and Uptime Kuma no longer fail with "see server logs".</p>
                  <p>Renaming your node now genuinely renames it everywhere: the machine's hostname, its .local network name (re-announced immediately), the local hosts file, and the HTTPS certificate all follow — so http and https links using your node's name keep working right after a rename.</p>
                  <p>The node no longer mistakes a VPN tunnel for its own address. On fresh installs with NetBird, apps could launch on an internal 10.x address instead of your LAN IP; the node now reads its address from the actual network route, fixing app launch links, generated app configs, and VPN setup.</p>
                  <p>Your cloud got a real layout: Apps-style tabs with categories for Folders, My Files, and Peer Files, readable file rows, a search that also finds files shared by your federated peer nodes — and music now opens in the bottom-bar player instead of a broken preview window.</p>
                  <p>The first-login experience flows again: the dashboard entrance animation is back — and you can actually hear it now (its sound was silently swallowed before, including on replays) — "Replay Intro" in Settings actually replays it, opening a direct link to an inner page no longer detours through the splash screen, the login screen keeps the intro video until your first login (switching to rotating backgrounds after), and the intro video streams three times lighter so it starts instantly.</p>
                  <p>Changing DNS settings no longer blanks the page, and the DNS and WiFi dialogs now cover the whole app instead of only the right panel.</p>
                  <p>The public demo is richer and truer: the intro plays on every fresh visit, Ark wallet flows, working DNS and Tor service management, and a library of peer content with previews that never break.</p>
                  <p>Assorted fixes: failed installs clean up after themselves properly, and the transactions view fits mobile screens (capped at 60% of the visible viewport).</p>
                </div>
              </div>
              <!-- v1.7.100-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.100-alpha</span>
                  <span class="text-xs text-white/40">July 14, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin now supports multiple versions of both Bitcoin Core and Bitcoin Knots: install the version you want, switch between them, pin a version, or let it auto-update — and switching is designed to be safe, with no surprise resyncs.</p>
                  <p>Lightning grew up: your LND wallet's recovery seed is captured at setup and kept as an encrypted backup you can reveal from Settings, there's a new Channels tab with a fee control when opening channels, and on-chain and Lightning balances now show side by side.</p>
                  <p>Installing Lightning (and other Bitcoin-dependent apps) on a fresh node no longer fails repeatedly — the node now waits until Bitcoin is genuinely ready to answer before starting them, and Bitcoin sizes its storage to your actual disk and its memory cache to your RAM, so small machines stop swapping and stalling.</p>
                  <p>The wallet understands more money: Cashu v4 tokens are supported, you can pay for a peer's files from either your Cashu or Fedimint ecash, and the Transactions view now shows your Lightning, Cashu, and Fedimint activity together — with a payment confirmation screen and an automatic refund if a purchase fails.</p>
                  <p>Mesh radios got a major upgrade: Meshtastic direct messages are now true end-to-end-encrypted radio messages that interoperate with off-the-shelf Meshtastic phone apps, your radio's region and a shared channel are provisioned automatically, and a new setup window appears when a radio is plugged in — with board pictures, full radio settings, and signal-strength indicators.</p>
                  <p>Reticulum joins as a third mesh radio protocol with RNode LoRa hardware support, including sending images and voice messages over the radio — and every chat message now carries a small pill showing how it travelled (Mesh, FIPS, or Tor).</p>
                  <p>Your node can manage an OpenWrt router: set up its internet uplink from the UI with a Wi-Fi network scan, turn it into a TollGate pay-for-Wi-Fi hotspot with a real captive portal, and sweep the router's earnings into your node's wallet. The gateway's status appears on the Home screen's Network tile.</p>
                  <p>Peering is now trust-aware: "Invite a Peer" grants view-only Observer access while "Link Your Nodes" grants Trusted access, incoming requests ask for your confirmation with an optional message, Node Visibility is a single clear switch plus a list of discoverable nodes you can peer with, and the Fleet view shows your trusted nodes' health.</p>
                  <p>Updates and apps are verified end-to-end: release updates are cryptographically signed and checked against a key baked into your node, app definitions arrive via the signed catalog, and container images are checked against trusted sources before anything installs or runs.</p>
                  <p>Dozens of reliability fixes: failed installs no longer leave phantom app cards, uninstalling can't hang forever, apps you stopped stay stopped, crashed apps heal themselves (even "running" containers whose process actually died), the login page no longer refresh-loops, and the mobile layout fits real phone screens instead of hiding the last row behind the browser bar.</p>
                  <p>Ask your node things over the radio: send "!archy" for node status with no AI involved, or "!ai &lt;your question&gt;" in a direct message for an AI answer that comes back on the same path it arrived — with a model dropdown (Haiku, Sonnet, or Opus) and an "always allow" list in the Mesh AI Assistant panel.</p>
                  <p>The off-grid mesh radio no longer posts cryptic identity codes ("ARCHY:") to the shared public channel every minute, and mesh contacts take care of themselves: new radios you hear are added automatically, "Clear All" really removes contacts (they return when in range), each contact shows a reachability dot, and the Peers list has a search box.</p>
                  <p>You can message standard meshcore phone apps and they can message you — readable text both ways, private replies instead of public-channel broadcasts. Federated Archipelago nodes now appear on the Mesh Map.</p>
                  <p>Apps open as an overlay on top of whatever page you're on, in every display mode, instead of yanking you to a different screen; the Services tab groups apps by category with proper icons.</p>
                  <p>BTCPay Server keeps its plugins across restarts, connects to your node's own LND out of the box, and its invoices stay payable over private Lightning channels.</p>
                  <p>Fedimint federations show up in Wallet Settings again (the client app's configuration error is fixed), and Wallet Settings has tabbed sections for Cashu and Fedimint.</p>
                  <p>The phone companion app can upload and download files, edit saved server entries, opens non-embeddable apps in an in-app browser, and got a proper round launcher icon.</p>
                  <p>Six placeholder "apps" that were just web bookmarks are gone from the store, the Bitcoin dashboard works fully offline, Gitea opens on the right port, and mempool, strfry, and Electrum stopped their restart loops.</p>
                  <p>Kiosk displays: HDMI audio no longer stutters, and a bad display-clone state no longer sticks after reboot.</p>
                  <p>Consistent dropdowns, toggles, tabs, and modal styling across the UI; in Mesh chat, scrolling the conversation no longer also scrolls the contact list; "App Updates" and "App Registry" sit directly under Account in Settings; and a fresh node no longer reinstalls apps just because their definition file exists on disk.</p>
                </div>
              </div>
              <!-- v1.7.99-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.99-alpha</span>
                  <span class="text-xs text-white/40">June 17, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Your node can now hold Fedimint ecash as well as Cashu. Wallet Settings now has tabbed sections for each: keep your list of trusted Cashu mints, or paste a Fedimint invite code to join a federation, and the home wallet card shows both your Cashu and Fedimint balances side by side. A new "Fedimint Client" app in the catalog powers the federation side.</p>
                  <p>You can now buy files shared by another node, right from their cloud. When you open a peer's paid file you get a simple "Buy this file" picker with several ways to pay — instantly from this node's ecash balance, from your node's own Lightning wallet, on-chain from your node, or by scanning a Lightning QR code with any outside wallet. Once payment settles, the file downloads automatically.</p>
                  <p>Your node can now act as an AI assistant on the off-grid mesh radio network. If your node has a local AI model available (via Ollama), other people on the mesh can ask it a question by starting their message with "!ai" and get an answer back over the radio — handy where there's no internet. A new Mesh assistant panel lets you turn this on or off and shows whether a local AI model was detected.</p>
                  <p>You can now view your node's 24-word recovery phrase whenever you need it. Settings has a new "Recovery phrase" option that, after you confirm your password (and 2FA code if you use one), reveals the words behind a tap-to-show blur with a copy button — so you can write them down and store them safely offline.</p>
                  <p>Setting up a brand-new node is smoother and less alarming. If the node is still starting up while you generate or confirm your recovery phrase, it now quietly waits and retries instead of flashing a scary error, and offers a clear "Try again" button only when something genuinely goes wrong. The final setup screen also shows a gentle "securing your private connection…" status that turns to "ready" on its own, so you can tell the encrypted transport is coming up rather than stuck.</p>
                  <p>The NetBird VPN app now actually logs in. It was failing to reach its sign-in screen because the dashboard needs a secure (HTTPS) connection that wasn't being provided; the node now serves it over HTTPS and opens it in a browser tab, so the login flow completes.</p>
                  <p>When you use your phone to remote-control a node's attached screen, two-finger scrolling now works inside apps and panels, not just the main page. And tapping an app that's meant to open in an external browser now hands the link to your phone to open there, instead of trying to open it on the (often unattended) attached display.</p>
                  <p>You can now choose whether your node shares Bitcoin block headers over the mesh. The Mesh Bitcoin panel has new switches to announce headers to peers and to accept headers from them, and your choices are remembered.</p>
                  <p>Version numbers now display cleanly everywhere. In a few places the interface was showing a doubled "v" (like "vv1.7.98"); it now always shows a single, tidy version label.</p>
                  <p>The "Back" buttons throughout the cloud and other detail screens now look and behave consistently on both desktop and mobile, including when browsing another node's files.</p>
                  <p>For advanced testing, Settings now includes an optional "update &amp; app source" choice between the usual trusted origin and an experimental peer-to-peer (DHT swarm) mode that pulls updates and app content from other nodes first, falling back to the origin automatically. The trusted origin remains the default.</p>
                </div>
              </div>
              <!-- v1.7.98-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.98-alpha</span>
                  <span class="text-xs text-white/40">June 16, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Apps that crash now recover on their own. Multi-part apps like Immich and IndeedHub could have one of their pieces stop and stay stopped until the whole node was rebooted; the node now checks every couple of minutes and restarts any crashed piece automatically (while still leaving apps you deliberately stopped alone).</p>
                  <p>The on-screen kiosk display can no longer slow the whole node down. On machines without a graphics chip the kiosk browser could spin a CPU core at full tilt, starving everything else (including the wallet, which then timed out); it's now capped and uses lighter rendering on those machines.</p>
                  <p>If an update download fails, you're taken back to the Download button to retry, instead of being stranded on an Install button for an update that didn't actually finish downloading.</p>
                  <p>Your node's identity is clearer and always visible: Settings now shows your Node DID on every node (it previously only appeared if your browser had cached it) plus your node's npub, both with copy buttons. There's also a terminal tool to cryptographically prove all your node's keys come from your one seed phrase.</p>
                  <p>The "all nodes over Tor" group chat sends quickly now — the "sending" spinner clears as soon as the reachable nodes have the message, instead of hanging on a slow or offline node.</p>
                  <p>Message notifications now have a close button and open the relevant chat when tapped.</p>
                  <p>The encrypted mesh transport (FIPS) turns itself on automatically after setup — no button to press — and connects to peers more reliably (it retries and keeps connections warm), so node-to-node features use the fast path more often instead of falling back to Tor.</p>
                  <p>Your chat history with other nodes is saved reliably and now encrypted on disk, so it survives restarts and updates and can't be read from a stolen drive (only clearing chat removes it).</p>
                  <p>Peer media shows a "connecting" loader before a video or audio file plays, and audio errors are accurate instead of blaming File Browser.</p>
                  <p>The Fedimint app now displays with its proper styling, and the Connected Nodes screen stays compact — it shows a few nodes and scrolls, you can tap a node to jump to it in Federation, or tap Message to open its chat.</p>
                  <p>App updates can now arrive on their own without waiting for a full system release, so individual apps can be improved and shipped faster.</p>
                </div>
              </div>
              <!-- v1.7.97-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.97-alpha</span>
                  <span class="text-xs text-white/40">June 16, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The Bitcoin sync status on the home screen no longer disappears for a moment when it refreshes. If the node was briefly busy, the panel used to vanish and pop back; it now stays put and simply shows "Updating…" until the next reading arrives, while a genuinely stopped node still correctly shows as not running.</p>
                  <p>Bitcoin sync progress on the home screen now updates more promptly, so the percentage and block height keep pace with the node instead of lagging behind.</p>
                  <p>The Lightning wallet "connect your wallet" screen loads its details and QR code again across all nodes, instead of failing to fetch them.</p>
                  <p>Your list of trusted nodes is now clean: the same node no longer appears several times under different names, and removed nodes stay removed. In chat, a node that previously showed up as two separate contacts now appears just once.</p>
                  <p>Browsing another node's cloud is smoother: music and video files from a peer now preview and play properly (including seeking partway through), and the connection now shows a small badge telling you whether it's using the fast encrypted mesh or the slower Tor network.</p>
                  <p>Opening "My Folders" in the cloud now shows a clear, friendly message when the file app isn't running, instead of a confusing error.</p>
                  <p>The Electrum server app opens on its own once it's ready, instead of sometimes leaving a loading spinner stuck on top of the screen.</p>
                  <p>The Fedimint app now displays with its proper styling and icons, instead of appearing unstyled with a missing image.</p>
                  <p>The Mempool app now connects to your Bitcoin node whether the node is Bitcoin Core or Bitcoin Knots, instead of only working with one of them.</p>
                  <p>Nodes start up cleanly after a reboot. On some boots the node's main service was trying to start before its data drive had finished mounting, so it failed and retried about twenty times over roughly five minutes — showing a wall of "Failed to start" messages — before finally coming up. It now waits for the data drive to be ready first, so it starts on the first try.</p>
                  <p>The background images throughout the interface now load faster — they've been made significantly smaller with no loss of quality.</p>
                </div>
              </div>
              <!-- v1.7.96-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.96-alpha</span>
                  <span class="text-xs text-white/40">June 15, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The screen attached to your node now shows the normal Archipelago interface and your dashboard after you sign in, instead of a separate, stripped-down grid of app icons that could appear in its place. That extra screen has been removed so the attached display matches what you see everywhere else.</p>
                  <p>On a brand-new node, the attached screen now walks through the same welcome and setup steps you'd see on a phone or laptop, and shows the normal sign-in screen once the node is set up — so the on-device display always matches the rest of the interface.</p>
                  <p>When adding a FIPS network anchor, you can now choose whether it connects over TCP (for a public anchor reached across the internet) or UDP (for one on your local network), instead of it always assuming the local-network option.</p>
                  <p>Behind the scenes, a new automated two-node test now exercises real node-to-node features — browsing another node's shared files and handling a removed node — against live nodes before each release, so node-to-node problems are caught earlier.</p>
                </div>
              </div>
              <!-- v1.7.95-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.95-alpha</span>
                  <span class="text-xs text-white/40">June 15, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Browsing another node's shared files now works over the fast encrypted mesh. Opening a peer's cloud could fail with a generic "Operation failed" message because the request for their file list wasn't permitted over the mesh and came back as "not found" — and it never retried over Tor. The mesh now serves the file list directly, and if a peer can't answer over the mesh the node automatically falls back to Tor instead of giving up.</p>
                  <p>Nodes you remove from your federation now stay removed. Previously a deleted node could quietly come back the next time you synced with another node that still listed it. Removed nodes are now remembered as removed and won't reappear on their own — only if you add them back yourself.</p>
                  <p>The app credentials pop-up now appears as a normal centred box with a dimmed background over the whole screen, instead of stretching to fill the entire screen.</p>
                </div>
              </div>
              <!-- v1.7.94-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.94-alpha</span>
                  <span class="text-xs text-white/40">June 15, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Your node now joins the private encrypted mesh network on its own. A wrong built-in setting meant nodes were quietly never reaching the shared mesh meeting point, so everything between nodes fell back to the slower Tor network. Every node now connects to the mesh automatically on startup, so node-to-node features like file sharing use the faster encrypted mesh first and only fall back to Tor when a peer is genuinely offline. (Confirmed live: a node with its mesh setting wiped re-connected to the mesh by itself within a second of starting.)</p>
                  <p>You can now bring the mesh networking software up to the latest stable version straight from the node, with one action — it fetches the new version, checks it's genuine before installing, and restarts the mesh on its own. (Confirmed live end to end: a node on an older build was upgraded to the current stable release and rejoined the mesh automatically.)</p>
                  <p>The Lightning wallet screen connects again on nodes where it was showing a "failed to fetch" error instead of your balance and channels. The wallet app and the node now talk to each other correctly, and the connection quietly repairs itself if its details drift after a restart.</p>
                </div>
              </div>
              <!-- v1.7.93-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.93-alpha</span>
                  <span class="text-xs text-white/40">June 14, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Receiving Bitcoin and Lightning works again on nodes where the Lightning wallet was stuck locked. After some updates the wallet could come back locked with a password the node no longer had, so "generate a receive address" kept failing with a "wallet is locked" message that nothing could clear. The node now detects this and repairs itself automatically.</p>
                  <p>Each node now secures its Lightning wallet with its own unique, randomly generated password instead of a shared built-in one, and remembers it safely so the wallet unlocks on its own after every restart or update — no more getting stuck locked.</p>
                  <p>If a wallet is found locked with an unrecoverable password, the node rebuilds it cleanly so Bitcoin and Lightning start working again. (On these early-access nodes the wallet holds no funds, so nothing is lost — a wallet locked with an unknown password was already inaccessible.)</p>
                  <p>The self-repair was validated end to end on live nodes: a stuck, locked wallet was detected, rebuilt, and came back unlocked on its own, and stayed unlocked across restarts.</p>
                </div>
              </div>
              <!-- v1.7.92-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.92-alpha</span>
                  <span class="text-xs text-white/40">June 14, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The Electrum server app no longer flashes a "can't connect, try again" error over its loading screen while it's still catching up. If ElectrumX is building its index or waiting on the Bitcoin node, you now just see the sync progress, and the app opens on its own once it's ready.</p>
                  <p>Behind the scenes, the reboot-survival test now confirms the whole system is genuinely healthy after a restart — every app reachable, updates not stuck, core services answering — instead of only checking that containers came back, so update-related problems are caught before shipping.</p>
                  <p>Settings → What's New now lists the notes for every recent release again. The screen had quietly fallen several versions behind, so the last eight releases of changes weren't showing up there — they're all back now, and a release check keeps it from drifting again.</p>
                </div>
              </div>
              <!-- v1.7.91-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.91-alpha</span>
                  <span class="text-xs text-white/40">June 14, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Apps you've installed now reliably show their "Open" button again. Some apps — including Jellyfin, BTCPay Server, Fedimint, Gitea and Portainer — were running fine but their launch link sometimes went missing, so there was no way to open them from the home screen. They now open correctly.</p>
                  <p>Receiving Bitcoin is more dependable: if the wallet's internal connection details drift after a restart, it now repairs them on its own, and any error it does hit is reported clearly instead of as a generic failure or a misleading "wallet locked" message.</p>
                  <p>Installing Bitcoin now sets itself up correctly without manual help — a security credential that could previously be missing and stop Bitcoin from starting is created automatically before it launches.</p>
                  <p>The Electrum server app is back on the home screen and can be launched again.</p>
                  <p>Behind the scenes, the release now runs an expanded automated test suite before shipping, so these kinds of issues are caught earlier.</p>
                </div>
              </div>
              <!-- v1.7.90-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.90-alpha</span>
                  <span class="text-xs text-white/40">June 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Generating a Bitcoin receive address works again — the wallet now requests the correct address type, fixing the "400 Bad Request" error when creating an address.</p>
                  <p>In the companion app, the on-screen pointer can now click into apps and type — including the app store search box — instead of clicks and keystrokes not reaching app content.</p>
                  <p>"Open in a new tab" from the companion app now opens the app in your phone's browser, instead of doing nothing. The normal mobile browser keeps working as before.</p>
                  <p>The login/credentials pop-up on phones is once again a centered, properly sized window rather than stretching the full height of the screen.</p>
                  <p>The Electrum server now recovers on its own if its index ever gets corrupted, and shows a clear progress screen (with percent complete and block height) while it builds its index, instead of a blank or broken page.</p>
                  <p>Software updates are more reliable on slow internet connections — downloads are given much more time to finish before giving up.</p>
                </div>
              </div>
              <!-- v1.7.89-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.89-alpha</span>
                  <span class="text-xs text-white/40">June 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The AI assistant looks the way it always did again: no extra back button or close button on phones, and the desktop view fills the whole screen without a gap at the bottom.</p>
                  <p>System updates are much more reliable: updates that previously got stuck partway or failed to install now complete cleanly, and a failed update can no longer block all future updates.</p>
                  <p>After an update, the system now checks itself correctly on every node type, so working updates are no longer mistakenly undone.</p>
                  <p>Generating a Bitcoin receive address works again on nodes where a network proxy previously got in the way.</p>
                  <p>The Lightning wallet now recovers and unlocks itself properly after restarts.</p>
                </div>
              </div>
              <!-- v1.7.88-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.88-alpha</span>
                  <span class="text-xs text-white/40">June 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>AIUI now loads immediately again instead of waiting on a production availability probe and cache-busted iframe URL, restoring the lighter launch behavior from before the regression.</p>
                  <p>Bitcoin receive now uses LND's GET-based newaddress flow with the native SegWit address type, fixing the 501 Method Not Allowed response from the previous POST attempt.</p>
                </div>
              </div>
              <!-- v1.7.87-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.87-alpha</span>
                  <span class="text-xs text-white/40">June 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin receive now calls LND's on-chain address endpoint with the correct REST method, and backend failures keep the specific address-generation error instead of collapsing into the generic operation-failed message.</p>
                  <p>App launch credential interstitials now render as true full-screen overlays, and the launcher loading indicator uses the neutral brand palette instead of a blue spinner.</p>
                </div>
              </div>
              <!-- v1.7.86-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.86-alpha</span>
                  <span class="text-xs text-white/40">June 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fleet now preserves the last known node list, alerts, and selection locally while telemetry refreshes in the background, so the dashboard no longer blanks on tab switches or update scans.</p>
                  <p>Connected nodes and identities now reuse their last loaded data instead of reloading the visible list every time the user revisits the tab.</p>
                  <p>The Fleet matrix and detail views now show actual node names and host information instead of raw node id prefixes.</p>
                  <p>The network map only redraws when its graph data actually changes, which stops the D3 scene from visually resetting on every refresh tick.</p>
                  <p>Mobile federation and system-update actions now stack full width, and the ElectrumX app health check allows a long startup window so slow sync nodes do not restart mid-index.</p>
                </div>
              </div>
              <!-- v1.7.85-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.85-alpha</span>
                  <span class="text-xs text-white/40">June 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>ElectrumX now runs with less cache pressure and more memory headroom, reducing the restart loop seen during sync catch-up.</p>
                  <p>Portainer is pinned to 2.19.4 instead of latest, avoiding schema-drift restarts from surprise image updates.</p>
                  <p>LND receive-address creation now asks for a native SegWit address and returns clearer wallet/readiness failures when an address is not available.</p>
                  <p>Fleet telemetry now carries server name, hostname, and server URL, and the Fleet dashboard shows those names instead of hashed node ids.</p>
                  <p>Trusted federation peers are still auto-added transitively, but the local node no longer imports itself back into the fleet list.</p>
                </div>
              </div>
              <!-- v1.7.84-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.84-alpha</span>
                  <span class="text-xs text-white/40">June 11, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin trusted-node relay approvals now generate restricted txrelay credentials and restart the active Bitcoin backend so the new RPC whitelist is live.</p>
                  <p>Bitcoin Core now matches Bitcoin Knots for restricted relay RPC support, including txrelay secret injection and sendrawtransaction-focused permissions.</p>
                  <p>The Bitcoin UI companion image is pinned for OTA updates, and container scanning now avoids getting stuck busy after timeout or error paths.</p>
                </div>
              </div>
              <!-- v1.7.83-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.83-alpha</span>
                  <span class="text-xs text-white/40">June 11, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>App launch metadata now follows typed manifest launch interfaces more consistently, keeping catalog entries aligned with their runtime ports and launch surfaces.</p>
                  <p>Unsupported app surfaces were removed from the release path, including revoked OnlyOffice metadata and the unvalidated Saleor surface.</p>
                  <p>Mobile and desktop app surfaces received release polish: stricter production build typing, safer mesh desktop/tablet scrolling, improved mobile app layout, and a Home system card link that goes directly to Monitoring.</p>
                  <p>The Bitcoin UI avoids false stale/reconnecting messages when fresh block snapshots advance, and deploy tooling now skips local scratch/upload artifacts while bounding optional IndeedHub fixups.</p>
                </div>
              </div>
              <!-- v1.7.82-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.82-alpha</span>
                  <span class="text-xs text-white/40">May 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Saleor storefront proxying forwarded the correct external host and media paths, fixing Server Actions origin checks and product image optimizer failures.</p>
                  <p>The storefront received an internal media origin so rewritten media URLs resolve inside the Podman network without exposing private API ports to browsers.</p>
                </div>
              </div>
              <!-- v1.7.81-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.81-alpha</span>
                  <span class="text-xs text-white/40">May 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Saleor storefront installs switched to the prebuilt registry image instead of building Next.js on-device, avoiding build failures during stack installation.</p>
                  <p>Existing Saleor stacks were repaired on adoption by recreating missing storefront containers, forcing the app to bind on all interfaces, and resolving nginx upstreams after restarts.</p>
                </div>
              </div>
              <!-- v1.7.80-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.80-alpha</span>
                  <span class="text-xs text-white/40">May 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Saleor storefront proxying falls back to the direct request scheme when forwarded protocol headers are absent, fixing direct local launches on port 9011.</p>
                  <p>Public proxy support remains intact by still honoring forwarded HTTPS headers for Nginx Proxy Manager domains.</p>
                </div>
              </div>
              <!-- v1.7.79-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.79-alpha</span>
                  <span class="text-xs text-white/40">May 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Saleor was published as a recommended commerce stack with storefront, dashboard, API, worker, database, cache, Mailpit, and Jaeger services.</p>
                  <p>Saleor launches opened the storefront while dashboard credentials stayed visible in Archipelago, and public storefront domains received same-origin GraphQL proxying.</p>
                </div>
              </div>
              <!-- v1.7.78-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.78-alpha</span>
                  <span class="text-xs text-white/40">May 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>NetBird launches now stay on the unified dashboard/proxy origin at port 8087 instead of following stale server URLs on 8086.</p>
                  <p>NetBird proxy routing no longer depends on a hard-coded rootless Podman gateway IP and now includes the upstream management proxy gRPC path.</p>
                  <p>Mobile credential prompts keep long credential lists scrollable and the Cancel/Continue buttons reachable in both My Apps and the mobile icon grid.</p>
                  <p>Android app-session popups hand external login/signup windows to the system browser instead of dropping them inside the WebView.</p>
                </div>
              </div>
              <!-- v1.7.77-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.77-alpha</span>
                  <span class="text-xs text-white/40">May 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Saleor first-use credentials are shown in Archipelago before launch and in App Details, instead of leaving users at an unexplained dashboard login.</p>
                  <p>NetBird embedded login now uses upstream-compatible signing-key behavior and sends ID tokens to the management API, fixing post-signup Unauthenticated states.</p>
                  <p>Transient unnamed Podman helper containers are hidden from My Apps so generated names no longer appear as user applications.</p>
                </div>
              </div>
              <!-- v1.7.76-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.76-alpha</span>
                  <span class="text-xs text-white/40">May 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Apps with generated first-use credentials now show them in Archipelago before launch and in App Details.</p>
                  <p>NetBird API and OAuth routes now proxy through the stable host-published server port, and the embedded IdP keeps upstream-compatible signing-key refresh settings while the dashboard sends ID tokens to the API so signup no longer lands in an Unauthenticated dashboard state.</p>
                  <p>Transient unnamed Podman helper containers created during app installs are hidden from My Apps, so random generated names like eager_keldysh no longer appear as applications.</p>
                  <p>Mobile App Store categories are now visible as horizontal chips above the tab bar, Discover is reachable on mobile, category choices update the actual view, and apps that require a real tab open directly from the icon tap.</p>
                </div>
              </div>
              <!-- v1.7.75-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.75-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>NetBird repair now rewrites the unified-origin config and recreates the browser-facing proxy/dashboard while preserving existing control-plane data.</p>
                  <p>Desktop dashboard scrolling hands focus back from the sidebar to the main content when the pointer or wheel moves over the main pane.</p>
                </div>
              </div>
              <!-- v1.7.74-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.74-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>App-session right panels now re-focus the iframe after load and when the frame area is activated, so scrolling works immediately after selecting an app or switching tabs on shorter screens.</p>
                  <p>NetBird now uses a unified local launch origin on port 8087 that serves the dashboard and proxies auth/API routes to the server, fixing the Unauthenticated and 404 logout/login loop.</p>
                  <p>Existing NetBird installs are repaired during adopt/start by rewriting the config files and creating the missing dashboard/proxy containers while preserving data.</p>
                </div>
              </div>
              <!-- v1.7.73-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.73-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Mobile apps that block iframe embedding now open directly in a browser tab instead of first landing in a broken in-shell webview.</p>
                  <p>App Store search covers all apps while searching, My Apps search can surface matching installable App Store entries, and mobile My Apps/Websites tab switching updates the view reliably.</p>
                  <p>NetBird installs prefer a 100.x tailnet address when available, and app sessions gained iframe auto-focus plus a scroll host for right-frame scrolling.</p>
                </div>
              </div>
              <!-- v1.7.72-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.72-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Settings What's New is caught up again. The modal now includes the missing entries for v1.7.68-alpha through v1.7.71-alpha instead of stopping at v1.7.67-alpha.</p>
                  <p>The release lockfile metadata is also kept in sync with the previous release bump.</p>
                </div>
              </div>
              <!-- v1.7.71-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.71-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>NetBird stack installs now create the exact persistent data directory before binding it into the server container, fixing the failed install path seen on the test node.</p>
                  <p>NetBird start and restart actions bring up the control-plane server before the dashboard, so lifecycle actions use the correct dependency order.</p>
                  <p>App-session fallbacks now return to My Apps under /dashboard, mobile iframe-blocked apps stay inside Archipelago with an explicit fallback, and installed Gitea containers show the packaged Gitea icon with rounder app icon masks.</p>
                </div>
              </div>
              <!-- v1.7.70-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.70-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>NetBird was corrected from the peer/client daemon image to the self-hosted control-plane stack, with a launchable dashboard on port 8087, management/signal/relay server on 8086, and STUN on UDP 3478.</p>
                  <p>Local app launches use direct host ports and carry an explicit dashboard return target, so closing an app session goes back to the launching dashboard screen instead of falling through to browser history or a 404.</p>
                  <p>Mobile launches ignore stale desktop panel state and route into the full app-session webview. The desktop sidebar also keeps top and bottom regions pinned while only the middle navigation scrolls on short screens.</p>
                </div>
              </div>
              <!-- v1.7.69-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.69-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>App installs now allow up to 10 minutes for slow initial install RPCs, matching large container pulls and preventing apps from disappearing from My Apps while the backend is still pulling or retrying mirrors.</p>
                  <p>Gitea is now categorized as a known Data app and stays visible during slow registry pulls. Live diagnostics confirmed the Gitea container came up healthy on port 3001 after the frontend had previously timed out too early.</p>
                  <p>NetBird was added to the catalog as a recommended networking app, and the Archipelago terminal includes nano on new installs and existing-node self-update fallback.</p>
                </div>
              </div>
              <!-- v1.7.68-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.68-alpha</span>
                  <span class="text-xs text-white/40">May 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>BTCPay Server now ships on the official btcpayserver image, fixing the plugin catalog crash caused by newer plugin dependency metadata while preserving existing data and Postgres databases.</p>
                  <p>BTCPay health checks no longer require curl inside the container, and Nginx Proxy Manager certificate challenge handling now avoids hijacking local API traffic while syncing issued public proxy hosts into host nginx.</p>
                  <p>System Update confirmation and mirror modals now cover the whole app, app-session close returns to the previous dashboard screen, and mobile app launches stay inside Archipelago's app-session webview.</p>
                </div>
              </div>
              <!-- v1.7.67-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.67-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Home status cards are calmer and more honest now. System, VPN, Bitcoin, and FIPS values keep their last known good state while route changes or short RPC failures are in flight, so the dashboard no longer flashes false "not configured" or "not running" states during normal refreshes.</p>
                  <p>Home, Web5 Monitoring, and the full Monitoring page now agree on the headline CPU, memory, disk, uptime, and load numbers. The UI uses one live system-stat snapshot for the visible cards while keeping the Monitoring page's historical store for charts, alerts, and container history.</p>
                  <p>The missing What's New history is filled in through this release, including every curated entry from v1.7.44-alpha through v1.7.66-alpha.</p>
                  <p>Bitcoin lifecycle specs are aligned again across Rust, first boot, and reconcile. Bitcoin Core/Knots get the intended memory headroom on normal hosts, and pruned Knots uses a larger dbcache when the node has enough RAM, improving IBD throughput without raising pressure on low-memory machines.</p>
                  <p>ElectrumX/electrs lifecycle specs now use the same memory policy everywhere, reducing drift between fresh installs, app lifecycle actions, and reconciliation.</p>
                </div>
              </div>
              <!-- v1.7.66-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.66-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nginx Proxy Manager stale-port repair now catches stopped or Created Podman records that still remember old port mappings. That means a stuck record can be removed and recreated before it blocks the current NPM ports.</p>
                  <p>Live recovery on the field node preserved the existing Nginx Proxy Manager data directory while recreating only the stale container metadata with the current 8081, 8084, and 8444 host ports.</p>
                </div>
              </div>
              <!-- v1.7.65-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.65-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Orchestrator-backed app starts now run the same pre-start repair path as the legacy Podman start flow. Nginx Proxy Manager can clean up stale port metadata before the orchestrator tries to bring it online.</p>
                  <p>Diagnostics confirmed host nginx was healthy while Nginx Proxy Manager itself had no listeners on its expected ports, narrowing the outage to NPM container lifecycle repair instead of the system proxy.</p>
                </div>
              </div>
              <!-- v1.7.64-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.64-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Authenticated update applies are no longer throttled so aggressively during troubleshooting. The System Update page now allows legitimate retry flows without immediately running into 429 Too Many Requests.</p>
                  <p>The release still includes the corrected backend rebuild protection, so OTA artifacts are built from the fresh Rust binary instead of an older compiled version.</p>
                </div>
              </div>
              <!-- v1.7.63-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.63-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The release script now rebuilds the backend after bumping the version and before hashing artifacts. OTA manifests no longer point at a stale backend binary.</p>
                  <p>This corrected the previous stale-artifact issue and carries the Nginx Proxy Manager stale-port repair in a backend binary that nodes can actually install and run.</p>
                </div>
              </div>
              <!-- v1.7.62-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.62-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nginx Proxy Manager start and restart now repair stale Podman containers that still publish the admin UI on host port 81, which conflicts with host nginx on updated nodes.</p>
                  <p>The repair recreates only NPM container metadata while preserving its persistent data and using the current 8081, 8084, and 8444 host mappings.</p>
                </div>
              </div>
              <!-- v1.7.61-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.61-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Multi-container stack installs stay in Installing for up to 20 minutes while dependency containers are being pulled and prepared. BTCPay no longer appears to vanish after two minutes while Postgres and NBXplorer are still being created.</p>
                  <p>Lifecycle stale-state recovery remains short for start, stop, restart, update, and removal actions, so genuinely wedged operations still clear quickly.</p>
                </div>
              </div>
              <!-- v1.7.60-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.60-alpha</span>
                  <span class="text-xs text-white/40">May 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Meshtastic serial detection now rejects malformed handshakes and skips known non-mesh serial devices such as Sierra Wireless LTE modems and Zooz/Z-Wave sticks.</p>
                  <p>Meshtastic config sync sends the correct protobuf wire type, allowing node-info and contact ingestion to work reliably. The mesh udev rule also stops claiming every ttyACM device and now targets known mesh adapters/vendors.</p>
                </div>
              </div>
              <!-- v1.7.59-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.59-alpha</span>
                  <span class="text-xs text-white/40">May 17, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Mobile app launching now keeps known container apps inside Archipelago's app-session flow instead of forcing desktop-only new-tab behavior.</p>
                  <p>App sessions on mobile respect the status-bar safe area, while the fullscreen backdrop remains edge-to-edge. The Apps page also gained a compact sideload button and modal for trusted Docker images.</p>
                  <p>Sideloaded app title and description metadata now persist through backend app config, and Meshtastic contact discovery retries config sync when the radio contact cache is empty.</p>
                </div>
              </div>
              <!-- v1.7.58-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.58-alpha</span>
                  <span class="text-xs text-white/40">May 17, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Mesh networking now supports Meshtastic radios over the serial API alongside existing MeshCore Companion USB radios. The shared listener probes preferred and auto-detected serial paths for both firmware families.</p>
                  <p>Meshtastic text packets are translated into Archipelago's existing mesh frame pipeline, and Meshtastic node information appears as normal mesh contacts using stable synthetic public keys.</p>
                  <p>Frontend OTA behavior improved: hashed assets no longer fall back to index.html, the HTML shell revalidates on every load, and runtime promotion installs the bundled nginx config on update.</p>
                </div>
              </div>
              <!-- v1.7.57-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.57-alpha</span>
                  <span class="text-xs text-white/40">May 17, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nginx Proxy Manager now avoids privileged rootless Podman host port 81, preferring 8081 for its admin UI while host nginx keeps a compatibility proxy on :81 for stale launch buttons.</p>
                  <p>App installs allocate ports by checking live host bind availability, falling back to a free high port when preferred ports are occupied. Portainer-created launchable containers now appear in a Websites tab through their discovered host ports.</p>
                </div>
              </div>
              <!-- v1.7.56-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.56-alpha</span>
                  <span class="text-xs text-white/40">May 15, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fresh installs include the full Wi-Fi userspace stack and grant the Archipelago service user NetworkManager PolicyKit access, so Intel Wi-Fi scanning and connection changes work from the web UI.</p>
                  <p>Container health and reconciliation are more honest and resilient: stale alerts clear, Stopping containers can be recreated, health states come from Podman, and drifted Quadlet settings trigger proper restarts.</p>
                  <p>Bitcoin Knots and ElectrumX get more CPU and memory headroom, LND helpers tolerate container-owned files better, and the screensaver stays out of media-heavy app sessions.</p>
                </div>
              </div>
              <!-- v1.7.55-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.55-alpha</span>
                  <span class="text-xs text-white/40">May 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Container reconcile can force-recreate Podman records stuck in Stopping while preserving bind-mounted app data, recovering wedged containers automatically.</p>
                  <p>Lifecycle audits on the hardened container layer passed on the validation node, with direct app probes returning healthy responses.</p>
                </div>
              </div>
              <!-- v1.7.54-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.54-alpha</span>
                  <span class="text-xs text-white/40">May 6, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Existing installs now self-repair nginx backend proxy locations for Bitcoin status and app catalog calls, including hosts where the active config is a copied file rather than a symlink.</p>
                  <p>LND UI is consistently served on port 18083 across first boot, Tor config, Quadlet reconciliation, OTA runtime payloads, and ISO scripts. OTA frontend tarballs also carry a cleaner runtime payload so startup promotion does not reintroduce stale host assets.</p>
                </div>
              </div>
              <!-- v1.7.53-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.53-alpha</span>
                  <span class="text-xs text-white/40">May 5, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin Knots/Core config generation no longer duplicates RPC bind and port settings between bitcoin.conf and container command arguments, fixing startup failures from RPC endpoint binding conflicts.</p>
                  <p>Legacy Bitcoin healthchecks no longer depend on bitcoin-cli being present in current images. Update checks now prefer manifest OTA releases over stale git remotes unless git updates are explicitly enabled.</p>
                </div>
              </div>
              <!-- v1.7.52-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.52-alpha</span>
                  <span class="text-xs text-white/40">May 5, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Tailscale now launches its local installed web UI on port 8240 and starts tailscaled before tailscale web, fixing unreachable installs after container creation.</p>
                  <p>Grafana lifecycle actions repair missing rootless host listeners on port 3000, and Debian 13 install paths pull security updates from trixie-security during image/install creation.</p>
                </div>
              </div>
              <!-- v1.7.49-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.49-alpha</span>
                  <span class="text-xs text-white/40">Apr 30, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin Knots/Core UI now reports connection, reconnecting, syncing, and error states from a backend status bridge instead of showing stale connection failures while the node warms up.</p>
                  <p>ElectrumX exposes indexed height, local Bitcoin height, known headers, status, and progress source, making long initial indexing states readable. Bitcoin Core and Bitcoin Knots are now mutually exclusive variants with corrected install conflict handling.</p>
                  <p>IndeeHub launches only on its direct web UI port, and BTCPay/NBXplorer Postgres environment formatting was fixed to avoid malformed connection strings.</p>
                </div>
              </div>
              <!-- v1.7.48-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.48-alpha</span>
                  <span class="text-xs text-white/40">Apr 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>archipelago.service now creates /run/containers before startup, fixing systemd mount-namespace failures on nodes where that runtime directory did not already exist.</p>
                </div>
              </div>
              <!-- v1.7.47-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.47-alpha</span>
                  <span class="text-xs text-white/40">Apr 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin Knots/Core sync is significantly faster: containers now use every available core for script verification and have 8 GB of memory so the 4 GB UTXO cache has headroom.</p>
                  <p>ElectrumX initial indexing is faster too, with CPU caps removed, 4 GB of container memory, and a 3 GB internal cache.</p>
                </div>
              </div>
              <!-- v1.7.46-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.46-alpha</span>
                  <span class="text-xs text-white/40">Apr 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Health monitoring no longer pages auto-restart failures for orphaned containers left behind after Bitcoin variant switches.</p>
                  <p>Apps no longer disappear from My Apps when an install fails, and multi-image stack pull progress now advances during the download phase instead of sticking at 20%.</p>
                  <p>Several docker.io images were mirrored into Archipelago registries, reducing first-boot install dependency on Docker Hub.</p>
                </div>
              </div>
              <!-- v1.7.45-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.45-alpha</span>
                  <span class="text-xs text-white/40">Apr 29, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin RPC auth is durable across container restart, image update, and reboot. The dashboard no longer fails because registry-pulled images shipped stale baked-in credentials.</p>
                  <p>Multi-container apps show real install progress, app cards stay visible while containers are being created, IndeedHub installs cleanly on fresh nodes, and Tailscale install no longer fails from a malformed command.</p>
                  <p>The installer now allocates swap on the encrypted data partition, capped at 8 GB, so image builds and memory spikes are less likely to OOM the system.</p>
                </div>
              </div>
              <!-- v1.7.44-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.44-alpha</span>
                  <span class="text-xs text-white/40">Apr 28, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Container orchestration migration and release hardening continued, including OTA synchronization for rebuilt UI containers and aligned LND UI port handling across runtime specs.</p>
                  <p>Release packaging moved toward tarball-only artifacts with archived ISO build recipes, keeping update payloads focused on the files existing nodes need.</p>
                </div>
              </div>
              <!-- v1.7.43-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.43-alpha</span>
                  <span class="text-xs text-white/40">Apr 23, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Installing, updating, and removing apps no longer freezes the UI. The backend now spawns the actual work in the background and returns immediately, so the progress bar starts moving right away instead of the whole page locking up for 30+ seconds while podman pulls an image.</p>
                  <p>Install progress bar actually reflects reality now. It previously stayed at 0% until the very end because podman doesn't emit parseable progress when run without a TTY. Replaced byte-counting with seven clearly-labelled phases — Preparing, Pulling image, Creating container, Starting, Waiting for health, Finalizing, Done — each mapped to a fixed percentage so the bar only moves forward.</p>
                  <p>Launch button now appears the moment an install finishes, instead of waiting up to 60 seconds for the next container scan. After a successful install or update, the backend kicks the scanner and waits for a fresh manifest to land before flipping the app to Running, so the UI always has real port and UI-route info by the time the card becomes clickable.</p>
                  <p>Retired the decommissioned Hetzner VPS mirror. New nodes default to the OVH mirror as Server 1 and tx1138 as Server 2 for both system updates and the app registry. Existing nodes auto-purge any saved entries for the dead mirror on next load so they stop paying connection-timeout penalties against a dead host.</p>
                  <p>Update-available badges and version comparisons work again across every app. The backend was looking for its pinned-image catalog at the wrong path and silently getting an empty result on deployed nodes, which meant the UI never showed "update available" even when a newer image was ready. The search path now matches where the image recipe actually installs the file.</p>
                  <p>Nodes with a 2 TB data drive are no longer silently configured as pruned Bitcoin nodes. The disk-size check that decides whether to enable pruning was measuring the tiny OS partition instead of the large encrypted data partition, so every archy install with a separate data volume was flipping into prune=550 mode on reconcile and deleting its historical blocks on the next bitcoin-knots restart. The check now measures the actual data partition, so full-archive nodes stay full-archive.</p>
                  <p>Recovery from a failed update no longer leaves a container permanently missing. When an app update failed partway through, the rollback path tried to restart the old container by name even though the forward path had already deleted it, leaving a hole in the node that required manual intervention. The reconcile tool now supports a --create-missing flag that rebuilds any registered container from its canonical spec, giving the update flow a safe recovery path.</p>
                  <p>OTA self-updates now refresh the container-reconcile helper scripts alongside the backend and frontend. Previously, updates only shipped new versions of the backend binary and web UI while the reconcile scripts stayed frozen at whatever version was baked into the original ISO — which meant fixes to those scripts (including the two above) never actually reached existing nodes. Every self-update now installs the latest reconcile-containers.sh, container-specs.sh, and image-versions.sh to /opt/archipelago/scripts/.</p>
                  <p>The container-install audit log is now actually written to disk. The backend runs as an unprivileged user and was trying to append every install, update, and lifecycle event to /var/log/archipelago-container-installs.log — a path only root can create. Every write failed silently, so the log stayed empty on every node. Logs now land at /var/log/archipelago/container-installs.log, a directory pre-created at boot and on self-update with the right ownership, and they rotate daily under the existing logrotate rule.</p>
                  <p>Mesh messages larger than one LoRa frame are no longer corrupted. The chunked-payload encoder was writing its 4-byte length header on top of the first 4 bytes of user data before running Reed-Solomon, so bytes 0 through 3 of every multi-chunk payload were lost in transit. The encoder now reserves the header up front, copies the data after it, and runs the forward-error-correction pass once, so chunked mesh payloads now round-trip intact.</p>
                  <p>Avatars no longer crash the backend on certain identities. The hue and accent colour computation multiplied a 16-bit seed byte by 360, which overflows for any seed value of 182 or greater. Debug builds panicked outright, release builds silently wrapped and drew the wrong colour. The math now runs in 32-bit space so every possible seed renders correctly.</p>
                  <p>Mesh outbox entries with a zero-second TTL now expire immediately instead of lingering forever. The expiry check used a strict greater-than comparison, so a message whose age had not yet ticked over one second was considered live even when its TTL was set to zero. It now uses greater-or-equal, matching the intuitive meaning of TTL.</p>
                  <p>The pinned-image parser no longer treats arbitrary environment-style keys as container images. The loader retained every key ending in _IMAGE regardless of the value, which meant a stray variable like NOT_AN_IMAGE="something" would be registered as a pinned image and pollute version checks. The parser now requires the value to look like an actual image reference (a registry path with a tag) before accepting it.</p>
                  <p>The AI Assistant tab no longer disappears after updates. Self-updates rebuilt the frontend from source and then used rsync --delete to swap it into place while passing --exclude aiui to preserve the existing bundle; that worked only as long as a previous install had already put AIUI on disk. Any node whose web UI directory got replaced wholesale (including by a manual redeploy of just the dist tarball) lost AIUI entirely and the AI Assistant tab fell through to a "needs to be enabled" placeholder. Every update and every ISO build now ships the canonical AIUI bundle from demo/aiui in the repo, so AIUI is a first-class versioned part of each release and cannot be wiped by a swap.</p>
                </div>
              </div>
              <!-- v1.7.42-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.42-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin dashboards no longer flicker errors during initial chain sync. When bitcoind is busy validating a fresh block it can take up to 10 seconds to answer RPC — the old code gave up after exactly 10 seconds, so any call that landed during that window surfaced as a failure even though the node was perfectly healthy. The RPC client now retries transient timeouts transparently (3 attempts, ~500ms + 1500ms backoff between them) and only surfaces errors that bitcoind itself reported. Connection refused is still fast-failed so genuinely-dead bitcoinds are reported in under a second.</p>
                </div>
              </div>
              <!-- v1.7.41-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.41-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Updates now self-check. After an update lands, the node probes its own web UI through nginx — if the frontend isn't answering cleanly within 90 seconds, the node automatically rolls back to the previous version and restarts. A bad release can no longer leave the fleet stranded on an unreachable node.</p>
                </div>
              </div>
              <!-- v1.7.40-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.40-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Proper fix for the 500 / Internal Server Error after update. The v1.7.38 and v1.7.39 frontend archives had the wrong permissions baked into the archive itself — the tarball's root directory entry was private, so every node that extracted it ended up with a web UI directory nginx couldn't read. v1.7.40 packages the archive with correct world-readable permissions from the start, so no node ever sees the 500 again.</p>
                </div>
              </div>
              <!-- v1.7.39-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.39-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Hotfix for v1.7.38 — on some nodes the update landed with the web UI directory set to private file permissions, so nginx returned a 500 / "Internal Server Error" on every page. This release fixes the updater to set world-readable permissions on the new frontend, and the node also now self-heals on boot if it ever finds the UI directory in that state again.</p>
                </div>
              </div>
              <!-- v1.7.38-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.38-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Signing in is quiet now. The intro music, welcome voice, and transition sounds belong to the first-boot cinematic and only play before you've finished onboarding — every login after that is silent. Typing sounds in the search bar and on the dashboard are unaffected.</p>
                  <p>Fixed a bug where clearing your browser cache, updating the node, or rebooting could bounce you back through the onboarding wizard even though your node was already fully set up. The node now self-heals: if your password is set, it knows you've been through onboarding and takes you straight to the login screen. No more starting over.</p>
                  <p>Trimmed the App Store. FIPS, Nostr Relay, Nostr VPN, Routstr, and Penpot have been removed from the catalog and their container images deleted from our registries. Your node's native FIPS transport is untouched — this is just the app-store entries going away.</p>
                </div>
              </div>
              <!-- v1.7.37-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.37-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin Core (the reference implementation) now installs from the App Store and runs cleanly alongside Bitcoin Knots as a first-class option. The install flow pulls the official docker.io/bitcoin image directly if your internal mirrors don't carry it, and the node UI auto-detects which implementation is running so the logo, title, and version line all reflect Core vs. Knots without any manual config.</p>
                  <p>The node dashboard now shows a Storage indicator (Full Archive · X GB or Pruned · X GB) right next to Network, so you can tell at a glance whether your node is carrying the full chain history or the last ~550 MB. The Node Settings modal was stripped of its hardcoded Regtest/port-18443 placeholders and now shows real values — network mode, storage mode, transaction index, ZMQ publishing, and RPC port — all read from the running node.</p>
                  <p>Fresh installs no longer default to pruned mode. Previously, a new install would write <code>prune=550</code> into bitcoin.conf even on boxes with 2 TB of free space; now the default is full archive and you can opt into pruning by editing the conf yourself.</p>
                </div>
              </div>
              <!-- v1.7.36-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.36-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Bitcoin Core joins the App Store as its own entry, with the Umbrel community icon and a description that frames it as a reference alternative to Bitcoin Knots rather than a replacement. A Sovereignty Stack tile on the Discover page now groups your node options together so the choice is obvious.</p>
                  <p>The App Store catalog fetch now follows whichever container registries you've set as primary in Settings. Previously the catalog URL was hardcoded to two servers; now the operator's own mirror priority drives where the App Store pulls its listings from, so switching primary actually moves the catalog too.</p>
                </div>
              </div>
              <!-- v1.7.35-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.35-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Rootless-netns self-heal: if the container network loses its outbound tap (symptom: Bitcoin Knots and other outbound containers can't reach the internet even though container-to-container still works), the node now detects it and restarts the network from scratch on its own. No more having to SSH in and bounce podman.</p>
                  <p>Every app card on the Apps page now has an Update button whenever a newer version of the app is available — same flow as the detail view, one click away. Updating apps used to require drilling into each card individually.</p>
                  <p>Your node's Web5 DID, Identities list, and peer-to-peer pubkey now all resolve to the same seed-derived identity instead of drifting apart after onboarding. The Node identity on fresh installs is mirrored from the onboarding seed rather than generated as a separate random keypair.</p>
                  <p>Bitcoin Knots (and the new Bitcoin Core slot) now run on bitcoin/bitcoin 28.4 with a realistic 4 GiB memory cap and uncapped CPUs so bitcoind can run -par=auto across every core on your box.</p>
                </div>
              </div>
              <!-- v1.7.34-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.34-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The login background now rotates through six atmospheric images, advancing one each time you land on the login screen, so returning to your node doesn't keep showing the same wallpaper. The chosen index is remembered across logouts.</p>
                  <p>Re-logging in is noticeably snappier. The dashboard entry animation used to replay the full 1.2-second zoom reveal on every login; that's now reserved for the first entry after onboarding. Subsequent logins fade in with just the welcome typing in about 300 ms.</p>
                  <p>If you clear site data on a node you've already onboarded, the intro video no longer fires again on the login screen. The onboarding cache is re-seeded from the backend automatically, so /login stays quiet instead of replaying the whole intro sequence.</p>
                </div>
              </div>
              <!-- v1.7.33-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.33-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The onboarding wizard no longer gets skipped on genuinely-fresh nodes when you connect from a browser that onboarded a different node earlier. The backend is now the source of truth for "has this node been onboarded yet?" — the browser's local flag is the offline fallback, not the primary answer.</p>
                  <p>Already-onboarded nodes no longer show the "boot loader" or "server starting up" screens during an OTA update blip. The health check polls quietly for up to a minute before showing the boot screen, so a 10-second restart no longer looks like a catastrophic failure.</p>
                  <p>Logging out and returning to <code>/login</code> no longer replays the full intro video — you get a quiet lock-screen background instead. The full welcome sequence is reserved for genuine first-time entries.</p>
                  <p>Upgrading nodes now pick up this release's UI cleanly without a stale cache hanging on. A cache-version bump tells your browser's service worker to ditch the old bundle on first load.</p>
                </div>
              </div>
              <!-- v1.7.32-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.32-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Hotfix: v1.7.31's frontend tarball was packaged with an extra wrapper directory, which left some nodes serving 403/500 after applying the update instead of the new UI. This release ships the tarball with the correct flat layout, and broken nodes heal automatically when this update applies.</p>
                  <p>Updates now finalize cleanly instead of being force-killed by systemd. Previously the node logged "shut down cleanly" during an update, then systemd waited 15 seconds and SIGKILL'd the service because one of the internal threads wasn't releasing. That's been tracked down and fixed, so the service exits promptly and the restart path is snappier.</p>
                </div>
              </div>
              <!-- v1.7.31-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.31-alpha</span>
                  <span class="text-xs text-white/40">Apr 22, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>IndeedHub install is now idempotent — re-running it after a failed first attempt no longer leaves orphaned containers blocking the retry with a "name already in use" error. The installer force-cleans leftover containers and the dedicated network before starting a fresh stack.</p>
                  <p>Server 3 (OVH) is now an automatic tertiary mirror for both system updates and app registries. Existing nodes pick it up on next restart without any manual config — another independent network path, so a single-provider outage can't stall downloads.</p>
                  <p>The reachability test on the Registries page no longer reports false "unreachable" for Gitea-backed registries. The probe now hits the Docker V2 API at the correct host-root path and accepts HTTP 405 in addition to 200/401 as "registry alive".</p>
                </div>
              </div>
              <!-- v1.7.30-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.30-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>App installs now show a real download progress bar — same accuracy as the system update bar. You'll see "Downloading: 50.5 / 200.0 MB (25%)" with a live percentage instead of a generic spinner. The bar keeps streaming even when the install falls back from one registry to another, so you'll never see a "stuck at 0%" again.</p>
                  <p>Uninstalls now show what's actually happening: "Stopping containers (2/5)", "Cleaning up volumes", "Removing app data" — labelled per app so you can fire off multiple uninstalls in parallel and watch each one's stage on its own card.</p>
                  <p>The OVH mirror is now baked in as Server 3 by default for both updates and the app registry — extra mirror, completely independent network path so a single-provider outage can't take everything down.</p>
                </div>
              </div>
              <!-- v1.7.29-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.29-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>New App registries page in Settings — same experience as Update mirrors, but for the container registries your node pulls app images from. Add a mirror, test reachability with one click, pick the primary.</p>
                  <p>New nodes default to the VPS registry as the primary for both app installs and the app catalog, with tx1138 as the automatic fallback if the VPS is slow or unreachable. Existing nodes keep whatever registry order they've already set.</p>
                  <p>App installs now genuinely honor the primary registry: the first pull attempt rewrites the image URL to use your primary, and only falls through to the secondary if that fails. Before, installs always hit whichever registry the image was hardcoded to.</p>
                  <p>Reboot screen now shows the animated "a" logo in the center of the ring — matching the screensaver's look so you get something nice to watch while the node comes back up.</p>
                </div>
              </div>
              <!-- v1.7.28-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.28-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Reboot now shows a proper progress screen. Click Reboot and you'll see a full-screen overlay with the familiar pulsing ring animation, a rebooting / reconnecting / back-online status, and an elapsed counter — no more black screen of mystery while you wait.</p>
                  <p>The overlay auto-reloads the page the moment your node is back up; if it takes longer than three minutes it surfaces a manual Reload button.</p>
                  <p>New nodes now default to the VPS mirror as Server 1 (primary) and tx1138 as Server 2 (fallback). Existing nodes keep whatever mirror order they've already set — use Set Primary on the System Update page to change it.</p>
                </div>
              </div>
              <!-- v1.7.27-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.27-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The Update page now shows which mirror delivered your update — a small "Served by" line under the new version tells you whether Server 1, Server 2, or a custom mirror was the one your node actually reached. Great for spot-checking that mirror fallback is doing its job.</p>
                  <p>Every mirror row has a new lightning-bolt button that pings the mirror and shows whether it's reachable, plus the round-trip latency in milliseconds. No more guessing if a mirror you just added is responding.</p>
                  <p>The Update mirrors section got a visual refresh: Set Primary, Remove, and the new Test action are compact icon buttons instead of crowded text, and adding a mirror now happens in a dedicated dialog that matches the rest of the UI.</p>
                </div>
              </div>
              <!-- v1.7.26-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.26-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Update downloads now have a mirror list. If the primary update server is slow or unreachable, your node automatically tries the next mirror and downloads the files from there — no more waiting on a stalled server with no recourse.</p>
                  <p>A new 'Update mirrors' section on the System Update page lets you see the list, add your own mirror URL, reorder which is tried first (Set primary), or remove one. The primary is tagged with a green PRIMARY pill.</p>
                  <p>Downloads automatically follow the mirror that served the manifest. Previously every mirror served the same manifest, and the manifest's download URLs were hardcoded to a single server — so even picking a faster mirror couldn't speed up the actual download. Now the backend rewrites download URLs to match whichever mirror succeeded.</p>
                  <p>Ships with two defaults: Server 1 (tx1138) and Server 2 (VPS). Add the URL format <code>https://host/.../releases/manifest.json</code> for custom mirrors.</p>
                </div>
              </div>
              <!-- v1.7.25-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.25-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Your node can now reach the broader FIPS public mesh, not just your own federated cluster. The FIPS daemon now binds both UDP (fast mesh forwarding) and TCP (NAT-friendly bootstrap) transports — matching the upstream factory default. The public anchor currently answers on TCP, so UDP-only nodes couldn't reach it; this fixes that without any action needed on your end.</p>
                  <p>Upgrading the config happens automatically. On next startup, if the installed FIPS yaml doesn't match the new two-transport schema, the node reinstalls and restarts the daemon so the TCP transport comes online. No manual Reconnect required.</p>
                  <p>Side benefit: TCP also helps on networks that block outbound UDP (corporate, some guest wifi) — your node falls back to TCP/8443 automatically and still joins the mesh.</p>
                </div>
              </div>
              <!-- v1.7.24-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.24-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Frontend updates now actually ship. Since roughly v1.7.17 the release pipeline had been rebuilding the backend every version but silently skipping the frontend bundle — a permissions issue on the build server meant vue-tsc failed before vite ever ran, and nobody noticed because the published tarballs still extracted cleanly. The result was the backend moving forward while the UI stayed frozen at its v1.7.9-era state, which is why the FIPS gear icon and the What's New entries for every release since then had been missing on your node.</p>
                  <p>Once this update applies, your node gets the real v1.7.24 frontend: the FIPS Seed Anchors modal (gear icon on the FIPS Mesh card), the current What's New history, the cancel-download button, and every other UI touch from the releases in between.</p>
                </div>
              </div>
              <!-- v1.7.23-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.23-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>FIPS Seed Anchors are now one click away. A small gear icon sits next to the status pill on the FIPS Mesh card — click it to open a modal where you can add, remove, and re-apply anchors. No more needing to go digging for the card or editing JSON by hand.</p>
                  <p>The modal lists each anchor with its label, truncated npub, address, and transport, plus an Apply button to force-redial the full list and a Remove button per entry. The add form right below validates that the address is host:port and the npub is bech32 before saving.</p>
                </div>
              </div>
              <!-- v1.7.22-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.22-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The FIPS Reconnect and Restart buttons now work on every node, regardless of which systemd unit is actually supervising the daemon. Previously they targeted only the archipelago-managed unit — nodes that were running the upstream unit instead saw the buttons silently do nothing. Both paths now auto-detect which unit is up and act on that one.</p>
                  <p>The FIPS anchor status no longer shows red just because one specific public anchor is unreachable. It now lights green whenever any authenticated peer is a recognised anchor — that's either the public anchor or something you added under Seed Anchors. A federated cluster that routes through its own seed anchor finally reports the truth.</p>
                  <p>Reconnect also re-pushes your seed anchors after the restart, so you don't have to wait five minutes for the background apply loop to re-dial them.</p>
                </div>
              </div>
              <!-- v1.7.21-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.21-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>FIPS bootstrap no longer depends on a single public anchor. You can now add your own anchors — other archipelago nodes or a VPS you control — and the node will dial every one of them to join the mesh on startup. If one anchor is down, the next one seeds the routing layer instead, so a flaky public anchor no longer strands a fresh install.</p>
                  <p>Anchors persist across restarts and are re-applied every five minutes, so a daemon that got temporarily isolated reconnects on its own without anyone having to SSH in. Each anchor carries an operator-editable label so you can remember which is which.</p>
                  <p>No behavior change if you don't configure any — the upstream daemon's own defaults keep working as before. This purely adds an operator-controlled list on top.</p>
                </div>
              </div>
              <!-- v1.7.20-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.20-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Fixed a critical bug where nodes on the automatic daily-update schedule could end up offline after their nightly update. The scheduler was killing the service a moment too early, before the built-in restart handler had a chance to bring the new version back up — leaving the node dead until someone SSH'd in and started it manually. The scheduler now hands off cleanly to the same restart path the 'Install Update' button uses, so auto-applied updates come back online on their own.</p>
                  <p>Applies to any node configured for 'Check &amp; Apply Daily' — no change required on your end, the fix ships with this update.</p>
                </div>
              </div>
              <!-- v1.7.19-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.19-alpha</span>
                  <span class="text-xs text-white/40">Apr 21, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Your node no longer offers a version you've already passed as an "available update". If you sideload or skip a release, any stored pointer to an earlier version is dropped on next restart, and the System Update page offers only the genuinely newer release — no more seeing an older version listed as something to install.</p>
                  <p>Version comparison is now numeric, not alphabetic. 1.7.10 correctly outranks 1.7.9 (earlier naive string-order would have got this backwards once the patch number hits double digits), so update prompts and "up to date" checks stay accurate past the nines.</p>
                  <p>A stale manifest from a slow cache or proxy can no longer downgrade your node. If the manifest reports a version equal to or behind what's running, your node treats that as "up to date" rather than offering the older version as an update.</p>
                </div>
              </div>
              <!-- v1.7.18-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.18-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nodes discovered through a trusted peer now land as Trusted instead of Observer. When your federated peer shares its own peer list with you, those nodes get the same trust level as a direct invite — the link they came through is already one you vetted, so you no longer need to promote them by hand before they can be used normally.</p>
                  <p>The update flow now writes clearer logs at every step. Start of download, cancel, and apply each emit a one-line entry to the system journal with the staging path and the affected files, so if a download misbehaves on your node it's easy to see exactly where it got to.</p>
                </div>
              </div>
              <!-- v1.7.17-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.17-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>When a download gets stuck, you can now cancel it. A new Cancel Download button sits next to the progress bar — it stops the transfer, clears the partial file, and returns you to a clean state so you can retry. No more staring at a frozen bar with no way to recover.</p>
                  <p>Downloads that stall for 30 seconds or more now say so. The progress bar turns amber and shows 'Download appears stuck — try Cancel and start again' instead of just sitting silently at whatever percent it reached.</p>
                  <p>Canceling is fast. It no longer has to wait out the retry timer — the download bails within half a second, so you're not stuck watching a stuck screen while you wait to unstick it.</p>
                </div>
              </div>
              <!-- v1.7.16-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.16-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Federation is now bidirectional and instant. When someone joins using your invite code, their node appears on your Federation page automatically — no need for the inviter to click Sync or wait for the next poll. Names and node details populate within seconds of the handshake finishing.</p>
                  <p>New nodes can no longer federate with themselves. Accepting an invite that points back at the local node (by DID, public key, or onion address) is rejected up front, so self-peering no longer clutters the node list with a duplicate card.</p>
                  <p>Transitive discovery: if nodes A and B are already federated and node C joins A, all three nodes now learn about each other. The new peer is pulled in as an Observer entry on existing federation members, so you can promote to Trusted with one click instead of trading a second invite code.</p>
                  <p>The Federation page auto-refreshes every five seconds while it's open. Status changes, new peers, and incoming join requests surface on their own — clicking Sync remains available for an on-demand pull.</p>
                </div>
              </div>
              <!-- v1.7.15-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.15-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Updates survive network hiccups. Downloads now resume from exactly where a dropped connection left off, and retry up to 6 times with increasing gaps between attempts, instead of restarting from byte zero or giving up.</p>
                  <p>The download progress bar now shows real progress. Instead of a fake number that creeps to 95% and freezes, you see the actual bytes arriving, and it continues to update correctly even if you navigate away and come back.</p>
                  <p>Update check itself retries on slow responses. If the release server is momentarily overloaded, the node tries three times with a five-second wait between attempts before concluding you're up to date.</p>
                </div>
              </div>
              <!-- v1.7.14-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.14-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Installing an update now shows a full-screen progress overlay with the Archipelago logo, a status message, and an animated bar. The page reloads itself automatically once the new version is up — no manual refresh. If something stalls, a 'Reload now' button appears after a few minutes.</p>
                  <p>Download progress no longer looks frozen near the end. The bar pauses at 95% with a 'Finishing download — verifying checksum…' message and spinner while the last bytes arrive and are hashed.</p>
                  <p>FIPS Reconnect now genuinely tries to fix the anchor. It runs a proper recovery sequence (stop → start → wait for the bootstrap window → check peers) and tells you the likely reason it's still unreachable — corrupt identity key, seed not unlocked, network blocking UDP, or the anchor server being down — instead of a generic 'try again'.</p>
                  <p>Healed a latent FIPS identity bug: the public-key file was being written in text form (an 'npub1…' string) on some nodes, which the daemon couldn't parse and silently authenticated with a garbage key. The Reconnect button now rewrites the file in the correct binary format and re-installs the config before restarting — nodes stuck with no peers for 'no reason' should come back online.</p>
                  <p>AIUI (Claude sidebar) is back. The installer now ships AIUI in the frontend bundle and preserves it across future updates — it was being wiped on every OTA because it lived outside the Vue build.</p>
                  <p>Installing a big app (IndeedHub, Bitcoin, Penpot) no longer gives up early and shows 'didn't work' while the download is still running in the background. The client waits up to 45 minutes for the install pipeline to finish.</p>
                  <p>'Rollback to Previous' is now labelled 'Rollback Available' — clearer that it's a choice you have, not a status you're stuck with.</p>
                </div>
              </div>
              <!-- v1.7.13-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.13-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>App catalog now loads reliably. Before, the Marketplace / Discover page couldn't fetch the catalog of apps because the upstream host wasn't sending the right CORS headers and the node's security policy didn't allow the fallback URL either. The node now fetches the catalog server-side and serves it same-origin to the browser — no more blank app lists.</p>
                </div>
              </div>
              <!-- v1.7.12-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.12-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Nothing new — version bump so freshly-installed nodes (from the 1.7.11 ISO) have something to OTA down, confirming the end-to-end update pipeline out of the box.</p>
                </div>
              </div>
              <!-- v1.7.11-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.11-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>OTA proof release — first version where Install Update should run clean from the UI with no manual steps. Click it and watch the sidebar flip to 1.7.11-alpha on its own.</p>
                </div>
              </div>
              <!-- v1.7.10-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.10-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Install Update actually applies now. The installer had to write into system folders that the backend service was sandboxed out of — every earlier 'Failed to apply update' was a layer of that onion. Fixed by running the file swaps in a separate system context.</p>
                  <p>FIPS status on the Home and Server pages now reflects whether the public anchor is reachable. You'll see 'Active · N peers' (green) when healthy or 'No anchor' (orange) when the network is blocking the bootstrap — same signal as the full FIPS card.</p>
                  <p>Pasting an https://… URL into the profile picture or banner now previews correctly. Before, if the URL failed to load, the UI would silently blank out instead of showing your initial as a placeholder.</p>
                  <p>Uploaded profile pictures under 64 KB are now embedded directly in your Nostr profile (as a data URL), so any Nostr client can see them — not just ones routing over Tor. Larger uploads keep the onion URL for now, with a hint to paste a public URL for wider visibility.</p>
                </div>
              </div>
              <!-- v1.7.9-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.9-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>OTA verification release — nothing new to see. Click Install Update, grab a coffee, and watch the sidebar flip to 1.7.9-alpha on its own. If this one works end to end, the pipeline is solid and future updates will flow the same way.</p>
                </div>
              </div>
              <!-- v1.7.8-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.8-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Install Update finally works end-to-end over the air. The installer was trying to overwrite the running backend binary with a tool that fails on in-use files (ETXTBSY) — swapped it for an atomic rename, which the kernel allows on a live executable. Every previous 'Failed to apply update' attempt was this one root cause.</p>
                </div>
              </div>
              <!-- v1.7.7-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.7-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Over-the-air update test — no feature changes, just a version bump so your node can walk through the whole update flow end-to-end using the new robust installer. Safe to apply; nothing to do afterwards.</p>
                </div>
              </div>
              <!-- v1.7.6-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.6-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Install Update is now more robust. Each install gets its own uniquely-named staging folder and then moves files into place — the previous version had a small cleanup step that could hit a transient filesystem hiccup and bail out halfway. You'll also still see a rollback folder after a successful install.</p>
                  <p>Dev-box OTA: nodes that build archipelago from source can now opt into the standard Download → Install flow instead of Pull &amp; Rebuild, by setting ARCHIPELAGO_UPDATE_URL in the service environment. Useful when the dev machine has a checked-out repo but you want to test the regular update path.</p>
                </div>
              </div>
              <!-- v1.7.5-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.5-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Over-the-air update test — no feature changes, just a fresh version number so your node can walk through the whole update flow end-to-end: check, download, install, auto-restart. Safe to apply; nothing to do afterwards.</p>
                </div>
              </div>
              <!-- v1.7.4-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.4-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Install Update actually installs now. Before, the final step extracted the new UI into the wrong folder and bailed with 'Failed to apply update' — your node ended up backing up cleanly but never swapping in the new files. Fixed.</p>
                  <p>Download progress no longer overshoots 100%. You'll see the bar climb smoothly to 95% and then jump to 100% when the download actually finishes.</p>
                </div>
              </div>
              <!-- v1.7.3-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.3-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>The version number in the sidebar now always matches the actual running version — no more lying to you about being on an older release after an update.</p>
                  <p>FIPS Mesh card on the server page: cleaner layout on desktop (no more awkward gaps), and a one-click Reconnect button when the public anchor is unreachable — it restarts the FIPS daemon so it can re-bootstrap from the anchor.</p>
                  <p>Profile pictures now show correctly in the identity list and editor. Before, uploaded images silently failed to render because the URL was only reachable over Tor; the UI now rewrites them to a local path while keeping the external URL for other Nostr clients.</p>
                  <p>Identity rows now show your Display Name first (from your Nostr profile) with the internal identity name beside it in parentheses, so you see the name other people will see — not just the one you picked when creating it.</p>
                </div>
              </div>
              <!-- v1.7.2-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.2-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Install Update now actually installs. Before, the button would back up your current version then fail with 'Failed to apply update' because the installer couldn't write into system folders.</p>
                  <p>The button's also been renamed to 'Install Update' (previously 'Apply Update') and the node restarts itself a moment after you click it — no more manual restart step.</p>
                  <p>Your existing identities now show the generated avatar instead of just their initials — same look as freshly created ones.</p>
                  <p>Everything from 1.7.0-alpha and 1.7.1-alpha carries over (default avatars on creation, one-click Save publishes to Nostr relays, public blob URLs for profile pictures, 30-minute download window, VPN peer restore on reboot, reconciler-only-repairs, filebrowser fix).</p>
                </div>
              </div>
              <!-- v1.7.1-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.1-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Over-the-air update test — same features as 1.7.0, just a fresh version number so your node can try the new download-and-apply flow end-to-end. Safe to apply; nothing to do afterwards.</p>
                </div>
              </div>
              <!-- v1.7.0-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.7.0-alpha</span>
                  <span class="text-xs text-white/40">Apr 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Every identity now gets a personal avatar the moment it's created. Your main node identity gets a distinctive hexagonal-network icon; other identities get a colourful generated pattern unique to each one.</p>
                  <p>Profile editor: upload a profile picture and a banner, then tap Save — your Nostr profile now goes out to the relays in one step. No more 'Save' vs 'Save &amp; Publish' confusion.</p>
                  <p>Profile pictures and banners you upload are now reachable by other Nostr clients across the network — not just your own browser. Anyone who sees your profile on a relay can load the image.</p>
                  <p>Update downloads on slow connections no longer cut out right at the end. The client waits up to 30 minutes for each component instead of giving up after 15 seconds.</p>
                  <p>When you move a node to a new version without going through Check for Updates (for example via a reinstall or manual copy), it now reports the new version correctly instead of endlessly saying 'update available'.</p>
                  <p>Your VPN peers come back automatically after a reboot. No more rescanning QR codes on your phone or laptop.</p>
                  <p>Fresh installs stay lean — only File Browser is included out of the box. Other apps wait in the Marketplace until you pick them.</p>
                  <p>File Browser stops rebooting itself every few hours — the housekeeper now leaves it alone once it's healthy.</p>
                  <p>One-click 'Pull &amp; Rebuild' button works for nodes that update from source (the development path), not just the standard download path.</p>
                  <p>The download progress number is now clean (like 45.23%) instead of 45.270894%.</p>
                </div>
              </div>
              <!-- v1.3.5 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.3.5</span>
                  <span class="text-xs text-white/40">Apr 11, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p>Migrated container registry to the self-hosted Gitea</p>
                </div>
              </div>
              <!-- v1.3.4 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.3.4</span>
                  <span class="text-xs text-white/40">Apr 7, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">NostrVPN — Built-in Mesh VPN</h4>
                    <p>NostrVPN is now a native system service. Peer discovery via Nostr relays, WireGuard tunnels. Auto-configured with your node's identity during onboarding — no setup required.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">New Marketplace Apps</h4>
                    <p>FIPS (encrypted mesh network) and Routstr (decentralized AI inference with Cashu payments) available as installable apps. Status UIs included for headless apps.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">ISO Boot Fix</h4>
                    <p>Fixed backend service crash on fresh ISO installs caused by overly restrictive systemd security settings. Fresh installs now boot cleanly.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">Bootstrap from Trusted Node</h4>
                    <p>New installs auto-connect to a trusted Bitcoin node via Tor during initial sync. ElectrumX, LND, and BTCPay work immediately while your local chain catches up.</p>
                  </div>
                </div>
              </div>
              <!-- v1.3.0 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.3.0</span>
                  <span class="text-xs text-white/40">Mar 19, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Full Security Audit</h4>
                    <p>33 security findings from a comprehensive penetration test — all fixed. Backend now only accessible through nginx. Path traversal, SSRF, and XSS vulnerabilities eliminated. Federation requires cryptographic signatures. Session tokens rotate after 2FA. Destructive operations now require password confirmation.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">Container Reliability</h4>
                    <p>Memory limits on every container prevent one app from crashing the whole system. Crashed apps now show a red "crashed" badge with a restart button instead of disappearing. Smart health status shows "starting up", "healthy", or "unhealthy" in real time. Apps you stop stay stopped — no more auto-restart fighting.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">Wallet on Home</h4>
                    <p>The Home dashboard now shows your Bitcoin wallet with on-chain, Lightning, and ecash balances. Send, receive, and view transaction history right from the home screen. New Transactions modal shows your full history with confirmations.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">LND Connect Fixed</h4>
                    <p>Connect Your Wallet (Zeus, Zap, BlueWallet) now works over both local network and Tor. QR codes generate correctly with REST API access.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">UI Polish</h4>
                    <p>Mesh view redesigned. New glass button styles throughout. Restart button on running apps. Improved app status badges. Cleaner navigation on the Apps page.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.9 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.9</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Security Hardening Complete</h4>
                    <p>All 12 pentest findings fixed. CSRF tokens now survive restarts. Password hashing upgraded to Argon2id. Bitcoin RPC gets a unique random password on every install. Federation messages require ed25519 signatures.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">7 Bugs Squashed</h4>
                    <p>Random logouts fixed (P0). Uninstall dialog is now a proper full-screen modal with an "Uninstalling..." overlay. App cards no longer flicker between Start/Launch during container scans. ElectrumX index estimate corrected.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">Bitcoin Sync on Dashboard</h4>
                    <p>Homepage System card now shows Bitcoin Core sync progress, block height, and green/orange status indicator when Bitcoin is running.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.8 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.8</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Pentest Remediation (9/12)</h4>
                    <p>Fixed 9 of 12 security findings: session auth on LND connect info, DEV_MODE removed from production, ed25519 signature verification on node messages, path traversal protection, NIP-07 origin validation, AIUI session checks, strict onion validation.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">UI Polish Batch</h4>
                    <p>Fedimint renamed to "Fedimint Guardian". Tab-launch icons. Marketplace sorts installed apps to end. Mesh mobile layout fixed. On-Chain first in receive modals. Federation shows names instead of DIDs. Cleaner iframe error screens.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.7 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.7</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Marketplace & Credentials</h4>
                    <p>29 containers running rootless. Marketplace app aliases working. Credential injection for inter-container authentication.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.4-6 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.4-6</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Rootless Podman Migration</h4>
                    <p>Migrated all containers from root to rootless Podman. UID namespace mapping, volume ownership fixes, sysctl tuning. Bitcoin RPC verified, all web services confirmed healthy. 29 containers up and running.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.2-3 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.2-3</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Systemd Hardening Restored</h4>
                    <p>Full systemd security sandbox restored now that containers run rootless. NoNewPrivileges, restricted namespaces, and system call filtering re-enabled. Session persistence and boot sequence fixes.</p>
                  </div>
                </div>
              </div>
              <!-- alpha.1 -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-white/10 text-white/60">v1.2.0-alpha.1</span>
                  <span class="text-xs text-white/40">Mar 18, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <div>
                    <h4 class="text-white font-medium mb-1">Mesh Radio & Container Stability</h4>
                    <p>LoRa mesh radio auto-detects USB port changes with a new Connect button. Fixed container crash loops — all apps start cleanly and stay stable. Apps starting up show progress instead of re-appearing in the store. Tor routing enabled by default for Bitcoin and Lightning.</p>
                  </div>
                  <div>
                    <h4 class="text-white font-medium mb-1">Off-Grid Bitcoin</h4>
                    <p>Receive Bitcoin block headers over mesh radio. Dead man's switch broadcasts location to trusted contacts if you go silent. GPS sharing is opt-in only.</p>
                  </div>
                </div>
              </div>
            </div>
            <button @click="showReleaseNotes = false" class="glass-button w-full mt-4 py-2 text-sm shrink-0">Close</button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- Session Card -->
    <div class="bg-black/20 rounded-xl px-5 py-4 border border-white/10 md:col-span-2">
      <div class="flex items-center gap-3 mb-2">
        <svg class="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">{{ t('settings.sessionStatus') }}</p>
      </div>
      <p class="text-base font-medium text-white/90">{{ t('settings.loggedIn') }}</p>
    </div>

    <!-- Identity Card: DID + npub + Tor Address -->
    <div v-if="userDid || userNpub || serverTorAddress" class="bg-black/20 rounded-xl px-5 py-4 border border-white/10 md:col-span-2 space-y-4">
      <div v-if="userDid">
        <div class="flex items-center justify-between gap-2 mb-2">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-amber-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
            <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">{{ t('settings.yourDid') }}</p>
          </div>
          <button
            @click="copyDid"
            class="shrink-0 px-3 py-1.5 rounded-lg glass-button glass-button-sm text-xs font-medium text-white/90 hover:text-white transition-colors flex items-center gap-1.5"
          >
            <svg v-if="!copiedDid" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
            <span v-else class="text-green-400 text-xs">{{ t('common.copied') }}</span>
            <span v-if="!copiedDid">{{ t('common.copy') }}</span>
          </button>
        </div>
        <p class="text-sm font-mono text-white/90 break-all" :title="userDid">{{ userDid }}</p>
        <p class="text-xs text-white/50 mt-1">{{ t('settings.didHelper') }}</p>
      </div>
      <div v-if="userNpub" :class="userDid ? 'pt-4 border-t border-white/10' : ''">
        <div class="flex items-center justify-between gap-2 mb-2">
          <div class="flex items-center gap-3 min-w-0">
            <svg class="w-5 h-5 text-purple-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
            <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">Node npub</p>
          </div>
          <button
            @click="copyNpub"
            class="shrink-0 px-3 py-1.5 rounded-lg glass-button glass-button-sm text-xs font-medium text-white/90 hover:text-white transition-colors flex items-center gap-1.5"
          >
            <svg v-if="!copiedNpub" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
            </svg>
            <span v-else class="text-green-400 text-xs">{{ t('common.copied') }}</span>
            <span v-if="!copiedNpub">{{ t('common.copy') }}</span>
          </button>
        </div>
        <p class="text-sm font-mono text-white/90 break-all" :title="userNpub">{{ userNpub }}</p>
        <p class="text-xs text-white/50 mt-1">Your node's Nostr public key, derived from its seed.</p>
      </div>
      <div v-if="serverTorAddress" :class="(userDid || userNpub) ? 'pt-4 border-t border-white/10' : ''">
        <div class="flex items-center gap-3 mb-2">
          <svg class="w-5 h-5 text-amber-400 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
          </svg>
          <p class="text-xs font-semibold text-white/60 uppercase tracking-wide">{{ t('settings.onionAddress') }}</p>
        </div>
        <p class="text-sm font-mono text-amber-400/90 break-all mb-1" :title="serverTorAddress">{{ serverTorAddress }}</p>
        <p class="text-xs text-white/50 mb-3">{{ t('settings.onionHelper') }}</p>
        <button
          @click="copyOnionAddress"
          class="w-full min-h-[44px] rounded-lg glass-button text-sm font-medium text-white/90 hover:text-white transition-colors flex items-center justify-center gap-2"
        >
          <svg v-if="!copiedOnion" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
          </svg>
          <span v-if="!copiedOnion">{{ t('common.copy') }}</span>
          <span v-else class="text-green-400">{{ t('common.copied') }}</span>
        </button>
      </div>
    </div>
  </div>
</template>
