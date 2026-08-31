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
              <!-- v1.8.6-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.6-alpha</span>
                  <span class="text-xs text-white/40">August 31, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Companion 0.5.28 is included in the node download this time, with the work that missed v1.8.5.</strong> The companion hub can back up and restore its node list, act as a NIP-46 remote signer, and shows each paired node's FIPS mesh address with tap-to-copy. For Termux users, the included fipssh helper turns a durable node npub into its mesh address, so fipssh user@npub1… can reach SSH once that node has explicitly allowed port 22. The node-side “SSH over mesh” firewall toggle is not claimed here—it still needs implementation and remains off by default.</p>
                  <p><strong>What's New now starts cleanly at v1.8.0 and is guaranteed to be newest-first.</strong> Older alpha history no longer overwhelms the useful recent changes, the three stray v1.7 entries that appeared above current releases are gone, and the release check now fails if either the ordering or the v1.8.0 history floor drifts again.</p>
                  <p><strong>A release can no longer advertise itself before its files exist.</strong> New releases are prepared behind a pending manifest; the publisher uploads the backend and frontend, downloads both back and verifies their size and hash, and only then promotes the signed manifest to the path nodes read. The manifest generator also includes every curated What's New item instead of silently stopping after the first ten physical changelog lines.</p>
                </div>
              </div>
              <!-- v1.8.5-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.5-alpha</span>
                  <span class="text-xs text-white/40">August 30, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Cuprate — an independent Monero node — is now an app.</strong> Monero consensus validated by a second, unrelated codebase (Rust), the same layer of security-in-depth Bitcoin gets from Knots. Review caught two problems before anything shipped: the unrestricted RPC that can move funds stayed bound to the container's loopback (never published to the node, let alone the LAN — anything on the node could previously have reached it), and its restricted RPC moved off port 18089 to avoid colliding with Penpot. Honest caveat: upstream has cut no stable release yet, so the pin tracks an exact preview build (0.1.0-preview-18-g618ff14) and moves to their first tagged release when there is one.</p>
                  <p><strong>A frozen node now explains itself — and comes back on its own.</strong> The host now captures a memory dump into /var/crash when the kernel panics <em>or</em> wedges (a hung kiosk used to sit dead until someone power-cycled it; now it dumps, reboots itself, and leaves the evidence behind), and records failing-memory signals (ECC errors) into a database as they happen. This is the first change delivered by a new host-update channel: the node's own updater now carries OS-level packages and settings to already-deployed machines — the crash-kernel's memory reservation is the one part that waits for a reboot, and the node says so rather than pretending.</p>
                  <p><strong>Uninstalling an app can no longer report success when it failed.</strong> The declarative path used to swallow every teardown error and report the app uninstalled, leaving the tile behind and the truth in the logs. A failed uninstall now stops and shows the real per-app errors, so "still there" is never presented as "gone".</p>
                  <p><strong>Pictures to internet-only mesh contacts work now.</strong> Sending an attachment inline always took the radio path and failed with "Peer is federation-only (no radio twin)" for contacts reachable only over the internet — and the size-adviser kept recommending a radio transfer those peers can't receive. Both fixed: inline sends route over the federation when that's the only way to reach the peer, and the advice no longer offers radio-only transfers to radio-unreachable contacts.</p>
                  <p><strong>Disk cleanup finally has honest numbers.</strong> Space "free" on a drive was counted including the slice the filesystem keeps reserved for root — roughly 5% of the disk, 92 GB on one dev box — so the automatic cleanup that's supposed to kick in at 90% never triggered and stale container images piled up unnoticed. Reserved space now counts as used, which is what the threshold was always meant to measure.</p>
                  <p><strong>Three small screens that were lying to you, fixed.</strong> The "Bitcoin is synced — fund your wallet" toast no longer appears on a node where the wallet it means (LND) isn't installed — it points at installing LND instead. The seed-reveal screen hides its third prompt unless the password actually fails to decrypt (the backup passphrase only exists if you set one). And multi-version store cards stop quoting a version number you'll be asked to choose on the next screen anyway.</p>
                  <p><strong>Mesh notifications survive a refresh, and a stale router no longer hides the fix.</strong> Radio message unread counts are now remembered per contact instead of guessed from session state (the "one new message showed 11 unread" bug), cover Meshtastic, MeshCore and Reticulum alike, and deep-link to the right conversation; a single new message announces itself once. Separately, when the cached router address goes stale, the error card gains a "Reconfigure router" action instead of a Retry loop that can never succeed.</p>
                  <p><strong>The app updater now knows what upstream shipped.</strong> Every app's manifest records where it comes from — including the odd corners (GitLab-only projects, ghcr-only images) — and a checker sweeps all of them against upstream releases, so a pin that quietly rots for months is now visible instead of invisible. The first full sweep found 27 pins behind; the safe patch-level ones shipped with this release (strfry, BTCPay Server 2.4.3, the two nginx frontends), and the major jumps that may carry data migrations are deliberately held for their own careful passes.</p>
                </div>
              </div>
              <!-- v1.8.4-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.4-alpha</span>
                  <span class="text-xs text-white/40">August 20, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Apps with their own login can now skip the node's login screen — Gitea and BTCPay Server do so out of the box.</strong> Some apps bring a complete account system of their own, and putting the node's password page in front of them broke real workflows: git clients can't answer a browser login, and a BTCPay checkout link handed to a customer must open for that customer. These apps are now served directly on their own login, while the node still fronts the connection for everything else it does (embedding fixes, the "app is restarting" page, Tor). Every app gets a new <strong>Settings → app → Access control</strong> switch, so you can put the node login back in front of any app — or take it away from one — with one click, effective immediately. App developers declare the default in their manifest (auth: open), documented in the developer guide.</p>
                  <p><strong>The phone remote now works inside apps on the TV — tap, scroll, and type everywhere.</strong> The companion remote and keyboard drove the dashboard beautifully but died at the edge of any app screen (Gitea, BTCPay, and friends): for the browser, each app is a separate website embedded in the page, and simulated input is forbidden from crossing that wall. The on-screen display now accepts the remote's input the way a real mouse and keyboard arrive — below the page, through the browser itself — so it lands anywhere on screen, app screens and tabs included. Taps click, two-finger scrolling scrolls the app, and typing goes into whichever field you tapped. Existing kiosks pick this up with the update, no reinstall needed.</p>
                  <p><strong>While you're driving with the phone remote, the old mouse pointer gets out of the way.</strong> The computer's own pointer used to sit frozen wherever the physical mouse last left it — a second, dead cursor next to the live orange one. It now hides while the remote is in use and returns half a minute after the last remote input.</p>
                  <p><strong>"Are you sure?" questions no longer freeze the remote.</strong> A handful of confirmations (clearing mesh history, rebooting, deleting a backup, uninstalling an app) used the browser's built-in popup, which stops the whole page — including remote input — until someone clicks it with a real mouse. From the couch, that meant asking a question you couldn't answer. All of them are now proper in-app windows in the house style, fully driveable by remote.</p>
                  <p><strong>A mesh radio now connects no matter which port it's plugged into — or replugged into.</strong> Moving a radio to a different USB port could leave the mesh silently down: the node only checked a short fixed list of port names (a radio landing outside it was invisible), a hand-set serial-port override quietly outranked the device you'd just approved in the "Radio detected" window, and one whole family of boards (Espressif-based radios like recent Heltec/T-Deck models) never received a stable device name at all — the exact combination found live on a fleet machine this week. All three are fixed: every serial port is scanned, choosing a radio in the detection window clears any stale override, and Espressif boards get the same stable name as everyone else.</p>
                  <p><strong>Mesh signal strength is honest now.</strong> Every peer heard over Reticulum radio reported a signal strength of exactly 0 — which is also what you'd see with no radio at all, and what peers reached over the internet showed. Real receptions now show their true signal reading, and anything that arrived over a relay or the internet says so by showing none — so "the radio is working" and "the internet is doing the radio's job" no longer look identical. (The reading depends on the radio's firmware reporting it; boards that don't report per-packet signal stats show "unknown" rather than a made-up number, and the new radio diagnostics show at a glance whether yours reports them.)</p>
                  <p><strong>A background error that repeated every 90 seconds, forever, is gone.</strong> After setting up a node from its recovery phrase, the node kept introducing itself to its federation partners with its old temporary identity papers while signing with its new ones — every partner rejected the introduction, and both sides logged an error about it every minute and a half until the next restart. The identity switch now updates everything at once, a rejected introduction is no longer misreported as delivered, and a partner who has already answered is no longer re-asked on every cycle.</p>
                </div>
              </div>
              <!-- v1.8.3-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.3-alpha</span>
                  <span class="text-xs text-white/40">August 14, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>The network map on TVs: no more blank page, no more frozen page — and it moves again.</strong> The map's entrance animation needed a smoothness that TV kiosk hardware can't always deliver, so the page could sit blank until a refresh; the previous fix cured the freeze by stopping the animation entirely, which went too far. Now the map appears instantly with everything already in place, then resumes its calm orbital motion at a gentler pace suited to TVs. Resizing or rotating any screen also redraws the map properly instead of leaving it tiny, stretched, or empty.</p>
                  <p><strong>The dashboard's corner logo is back to normal.</strong> The new glossy paint finish was meant for the big emblem on the screensaver, intro, and login screens — it had quietly spread to the small logo in the dashboard header, where it looked wrong. Each screen now gets exactly the treatment intended for it.</p>
                  <p><strong>App icons no longer vanish in My Apps.</strong> The freshly restyled Alby Hub and phoenixd icons could render as blank squares in some views — a subtlety in how the icon files declared their size. Fixed at the source, and the icon tool app developers use now produces immune files.</p>
                </div>
              </div>
              <!-- v1.8.2-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.2-alpha</span>
                  <span class="text-xs text-white/40">August 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>An app that can't be shown inside the dashboard now becomes a tab app by itself.</strong> A few apps refuse to render inside another page no matter what — they break out with their own code or insist on owning the whole browser window. Opening one used to mean staring at a grey pane. Now the dashboard notices, offers the app in its own tab, and remembers: from then on that app's button opens a tab directly (with the little launch icon that tab apps carry), first click, every time. If a later update makes the app embeddable after all, the dashboard notices that too and goes back to embedding it.</p>
                  <p><strong>The logo emblem got its glossy black paint finish — properly this time.</strong> The circle behind the A on the screensaver, intro, and login now wears a deep wet-paint look: warm light blooming from the top edge, fine grain so the dark tones stay smooth instead of banding, and no more ring border. (An earlier rougher version of this experiment briefly shipped by accident and then vanished depending on which screen you were on — this is the finished, deliberate one, everywhere.)</p>
                  <p><strong>New app icons now match the store's look, on every screen.</strong> Alby Hub and phoenixd arrived with edge-to-edge logos that ignored the breathing room every other app icon has, and the app detail page skipped the icon backdrop entirely. Both icons are re-set on the standard canvas, the detail page now applies the same icon treatment as the store tiles, and app developers get a one-command tool that puts any logo onto the house canvas automatically.</p>
                </div>
              </div>
              <!-- v1.8.1-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.1-alpha</span>
                  <span class="text-xs text-white/40">August 13, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Apps that refused to open inside the dashboard now embed like everything else.</strong> Some apps ship browser headers that forbid being shown inside another page — correct hardening on the open web, but inside Archipelago it produced a dead grey pane when you opened them from My Apps (Alby Hub was the first to hit it). The app gate, which already checks your login on every request to an app, now removes just those framing headers on the way through; each app's own content-security rules pass through untouched. No more per-app proxy workarounds.</p>
                  <p><strong>The network map no longer freezes kiosk TVs.</strong> The animated federation map at 4K was too much for the deliberately conservative graphics settings the on-screen display used on every machine — settings chosen years back to stop audio crackle on much older hardware. Two fixes: on kiosk screens the map now opens in its flat 2D view (the 3D globe is one tap away, and remembered) and animates at half rate — invisible from the couch, half the work. And the display itself now recognizes what machine it runs on: older kiosk boxes keep the proven careful settings, modern ones finally get real GPU rendering.</p>
                  <p><strong>New Settings → Display → Graphics choice for the on-screen display.</strong> Auto (recommended) picks the right rendering mode for the machine by itself; Compatibility forces the most conservative mode if a screen ever stutters, tears, or crackles; Quality forces full GPU rendering on hardware the automatic detection doesn't recognize. Changing it restarts the on-screen display, like the size presets.</p>
                </div>
              </div>
              <!-- v1.8.0-alpha -->
              <div>
                <div class="flex items-center gap-2 mb-3">
                  <span class="text-xs font-mono px-2 py-0.5 rounded bg-orange-500/20 text-orange-300">v1.8.0-alpha</span>
                  <span class="text-xs text-white/40">August 12, 2026</span>
                </div>
                <div class="space-y-3 text-sm text-white/80 pl-3 border-l border-white/10">
                  <p><strong>Archipelago is now open source.</strong> The full source code of the node you are running — the orchestrator, the dashboard, the app platform, the mesh, the release tooling — is published for anyone to read, build and audit at source.archipelago-foundation.org/lfg2025/archy. A node that holds your money, your files and your communications should not ask to be taken on faith: from this release onward you, or anyone you trust, can see exactly what it does and follow every change we make in the open.</p>
                  <p><strong>Installing an update is reliable again, and tells you what happened when it isn't.</strong> Some nodes could download an update but never apply it — the button stayed on "Install", and no amount of retrying worked. The cause: applying the update consumed the downloaded files as it went, so if any one step hit a snag partway through, the leftover files were incomplete and every later attempt failed the safety re-check forever, needing a technician to recover. Applying no longer consumes the download — a failed apply can always be retried from the same files — and the pieces are now applied in a fixed order with the program itself last, so a hiccup can't leave a half-swapped node. When an apply does fail, the screen now shows the real reason and what to do ("download the update again"), and offers Download again instead of a dead "Install" button, rather than a generic "it failed".</p>
                  <p><strong>Video on the kiosk stops tearing.</strong> The kiosk's display had no vertical sync at all, so fast motion — IndeedHub films especially — showed horizontal tearing lines. The display driver now syncs every frame to the panel (no extra hardware needed, existing kiosks pick it up with this update), and on machines with a GPU, video decoding moves off the CPU onto the video hardware — smoother playback that also leaves more headroom for audio, not less.</p>
                  <p><strong>The Back button finally does what you expect.</strong> Pressing Back — the mouse's side button on a kiosk, a swipe on a phone, the toolbar button in any browser — used to navigate the screen underneath an open window, or leave the dashboard entirely. Back now closes the topmost open window first, one per press, exactly like a native app; closing a window yourself never leaves a phantom entry that makes you press Back twice.</p>
                  <p><strong>No more bare IP addresses in your update or app-registry settings.</strong> The update mirrors and the app registry each listed the same server twice — once by its proper name, once as a raw http://146… address left over from before the domain existed. The raw-address entries are retired: new nodes never see them, and existing nodes clean them out of their saved lists automatically on the next read. Everything now goes through the named, TLS-protected origin — which was always the same machine.</p>
                  <p><strong>The phone companion app downloads over the proper domain.</strong> The download QR pointed at a raw address over plain HTTP; it now points at the same file on the https domain. Scanning it gets you an encrypted download from a named server.</p>
                  <p><strong>The Receive window now tells you when the money is on its way.</strong> Previously it showed a QR code and left you to check elsewhere whether anything happened. Now, the moment the sender's transaction is broadcast, the QR gives way to a clock: the amount, the transaction ID (tap to copy), and a note that the funds arrive on their own — with a single Done button. If you keep the window open, the clock becomes a green check at the first confirmation. Verified live on a real node: payment detected within seconds of broadcast.</p>
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
