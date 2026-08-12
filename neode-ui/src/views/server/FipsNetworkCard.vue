<template>
  <div data-controller-container tabindex="0" class="glass-card p-6 flex flex-col transition-all hover:-translate-y-1">
    <div class="flex items-start gap-4 mb-4 shrink-0">
      <div class="flex-shrink-0 w-12 h-12 rounded-lg bg-white/10 flex items-center justify-center">
        <svg class="w-6 h-6 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
      </div>
      <div class="flex-1">
        <div class="flex items-start justify-between gap-4 mb-2">
          <h2 class="text-xl font-semibold text-white">F*ck IPs Mesh</h2>
          <div class="flex items-center gap-3">
            <div class="flex items-center gap-2" :title="statusLabel">
              <span class="w-2 h-2 rounded-full" :class="statusDotColor"></span>
              <span class="text-sm font-medium" :class="statusTextColor">{{ statusLabel }}</span>
            </div>
            <button
              type="button"
              class="p-1.5 rounded-md text-white/50 hover:text-white hover:bg-white/10 transition-colors"
              title="Seed anchors"
              aria-label="Open FIPS seed anchors settings"
              @click="showAnchorsModal = true"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
            </button>
          </div>
        </div>
        <p class="text-white/70 text-sm mb-4">Fast Nostr-keyed mesh routing</p>
      </div>
    </div>

    <!-- Seed anchors modal — operator-editable list of peers this node
         dials to bootstrap the mesh. Tucked behind the gear so it
         doesn't crowd the card but is still one click away. Close
         button and layout mirror the What's New modal (and the rest
         of the app) so it feels like a first-class modal. -->
    <Teleport to="body">
      <Transition name="fade">
        <div
          v-if="showAnchorsModal"
          class="fixed inset-0 z-[3000] flex items-center justify-center p-4"
          @click="showAnchorsModal = false"
        >
          <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
          <div
            class="relative z-10 max-w-xl w-full"
            style="max-height: 90vh; overflow-y: auto"
            @click.stop
          >
            <FipsSeedAnchorsCard closable @close="showAnchorsModal = false" />
          </div>
        </div>
      </Transition>
    </Teleport>

    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-3 shrink-0">
      <div class="p-3 bg-white/5 rounded-lg">
        <p class="text-xs text-white/60 mb-1">Daemon version</p>
        <p class="text-sm font-medium text-white break-all">{{ status.version || '—' }}</p>
        <p v-if="!status.installed" class="text-xs text-white/40 mt-1">Package not installed</p>
      </div>
      <div class="p-3 bg-white/5 rounded-lg">
        <div class="flex items-center justify-between gap-2 mb-1">
          <p class="text-xs text-white/60">FIPS npub</p>
          <button
            v-if="status.npub"
            type="button"
            class="text-xs text-white/60 hover:text-white transition-colors flex items-center gap-1"
            :title="copied ? 'Copied!' : 'Copy full npub to clipboard'"
            @click="copyNpub"
          >
            <svg v-if="!copied" class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" /></svg>
            <svg v-else class="w-3.5 h-3.5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" /></svg>
            <span :class="{ 'text-green-400': copied }">{{ copied ? 'Copied' : 'Copy' }}</span>
          </button>
        </div>
        <p class="text-sm font-mono text-white break-all select-all">{{ npubDisplay }}</p>
        <p v-if="!status.key_present && status.npub" class="text-xs text-white/40 mt-1">Upstream key (not seed-derived)</p>
        <p v-else-if="!status.key_present" class="text-xs text-white/40 mt-1">Unlock seed to derive archipelago-managed key</p>
      </div>
    </div>

    <!-- Anchor status: always full-width to keep desktop layout tidy -->
    <div v-if="status.service_active" class="p-3 bg-white/5 rounded-lg mb-3 shrink-0">
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <div class="flex items-center gap-2 text-xs">
          <span class="w-2 h-2 rounded-full" :class="status.anchor_connected ? 'bg-cyan-400' : 'bg-orange-400'"></span>
          <span class="text-white/70">Anchor:</span>
          <span :class="status.anchor_connected ? 'text-cyan-300' : 'text-orange-300'">
            {{ status.anchor_connected ? 'connected' : 'not reached' }}
          </span>
          <span class="text-white/40">·</span>
          <span class="text-white/60">{{ status.authenticated_peer_count ?? 0 }} peer{{ (status.authenticated_peer_count ?? 0) === 1 ? '' : 's' }}</span>
        </div>
        <button
          v-if="!status.anchor_connected"
          type="button"
          class="text-xs px-3 py-1.5 rounded-md bg-orange-400/15 hover:bg-orange-400/25 text-orange-200 disabled:opacity-60 transition-colors"
          :disabled="reconnecting"
          @click="reconnectAnchor"
        >
          {{ reconnecting ? 'Reconnecting…' : 'Reconnect' }}
        </button>
      </div>
      <p v-if="!status.anchor_connected" class="mt-2 text-[11px] text-white/40 leading-snug">
        No known anchor is currently an authenticated peer. DHT routing to unknown npubs can't bootstrap; federation and messaging fall back to Tor until one reconnects. Reconnect restarts the FIPS daemon, which usually clears a stale identity cache. Add a cluster-local anchor in Seed Anchors if the public one is unreachable.
      </p>
    </div>

    <div v-if="statusMessage" class="mb-3 p-3 rounded-lg text-xs" :class="statusIsError ? 'bg-red-400/10 text-red-300' : 'bg-green-400/10 text-green-300'">{{ statusMessage }}</div>

    <div v-if="status.key_present && !status.service_active" class="flex gap-2 mt-auto pt-3 shrink-0">
      <button class="flex-1 min-h-[44px] px-4 py-2 glass-button rounded-lg text-sm font-medium transition-colors" :disabled="installing" @click="installAndActivate">{{ installing ? 'Starting…' : 'Start' }}</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, onActivated, onDeactivated, ref } from 'vue'
import { rpcClient } from '@/api/rpc-client'
import { useCachedResource } from '@/composables/useCachedResource'
import { safeClipboardWrite } from '@/views/web5/utils'
import FipsSeedAnchorsCard from './FipsSeedAnchorsCard.vue'

interface FipsStatus {
  installed: boolean
  version: string | null
  service_state: string
  upstream_service_state: string
  service_active: boolean
  key_present: boolean
  npub: string | null
  authenticated_peer_count?: number
  anchor_connected?: boolean
}

// Shares `server.fips-summary` with the Local Network card's FIPS row, so
// both paint from the same cache instantly on revisit and never disagree.
// persist:false (T-02-01) — the response carries `npub`, this node's own
// FIPS identity public key; must match Server.vue's persist decision for
// this same key exactly (a mismatch would trip the dev-only entry() warning
// and leave it ambiguous which decision actually governs the shared cache).
const statusRes = useCachedResource<FipsStatus>({
  key: 'server.fips-summary',
  ttlMs: 15_000,
  persist: false,
  fetcher: (signal) => rpcClient.call<FipsStatus>({ method: 'fips.status', signal, dedup: true, maxRetries: 1 }),
})
const status = computed<FipsStatus>(() => statusRes.data.value ?? {
  installed: false,
  version: null,
  service_state: 'unknown',
  upstream_service_state: 'unknown',
  service_active: false,
  key_present: false,
  npub: null,
  authenticated_peer_count: 0,
  anchor_connected: false,
})
const installing = ref(false)
const reconnecting = ref(false)
const statusMessage = ref('')
const statusIsError = ref(false)
const copied = ref(false)
const showAnchorsModal = ref(false)

async function copyNpub() {
  if (!status.value.npub) return
  try {
    await safeClipboardWrite(status.value.npub)
    copied.value = true
    setTimeout(() => { copied.value = false }, 2000)
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Copy failed: ${msg}`, true)
  }
}

const statusLabel = computed(() => {
  if (!status.value.installed) return 'not installed'
  // Active takes precedence: the daemon may be running from its own upstream
  // key on a legacy/dev node that doesn't have a seed-derived archipelago key.
  if (status.value.service_active) return 'active'
  if (!status.value.key_present) return 'awaiting seed'
  return status.value.service_state
})

const statusDotColor = computed(() => {
  if (status.value.service_active) return 'bg-green-400'
  if (!status.value.installed || !status.value.key_present) return 'bg-white/30'
  return 'bg-orange-400'
})

const statusTextColor = computed(() => {
  if (status.value.service_active) return 'text-green-400'
  if (!status.value.installed || !status.value.key_present) return 'text-white/50'
  return 'text-orange-400'
})

const npubDisplay = computed(() => {
  const n = status.value.npub
  if (!n) return '—'
  return n.length > 20 ? `${n.slice(0, 12)}…${n.slice(-6)}` : n
})

function flash(msg: string, isError = false) {
  statusMessage.value = msg
  statusIsError.value = isError
  setTimeout(() => { statusMessage.value = '' }, 6000)
}

async function installAndActivate() {
  installing.value = true
  try {
    const next = await rpcClient.call<FipsStatus>({ method: 'fips.install' })
    statusRes.optimistic(() => next) // confirmed server state, not a guess
    flash('FIPS started')
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Start failed: ${msg}`, true)
  } finally {
    installing.value = false
  }
}

// Restart the FIPS daemon and wait for the anchor bootstrap window.
// The backend runs a proper recovery sequence (stop → start → wait →
// classify) and returns a structured diagnostic we can show the user
// instead of a generic "still unreachable".
async function reconnectAnchor() {
  reconnecting.value = true
  try {
    const res = await rpcClient.call<{
      recovered: boolean
      likely_cause: string
      hint: string
      after: FipsStatus
    }>({ method: 'fips.reconnect', timeout: 60_000 })
    // Update the card with the post-reconnect status returned by the
    // backend — avoids an extra status fetch race.
    statusRes.optimistic((cur) => ({ ...(cur ?? status.value), ...res.after }))
    if (res.recovered) {
      flash('Anchor reconnected.')
    } else if (res.likely_cause === 'connected') {
      // Already connected, not a "recovery" per se.
      flash('Anchor is reachable.')
    } else {
      // Surface the backend's diagnostic hint verbatim — it's been
      // written for the fleet reader.
      flash(res.hint || 'Reconnect finished but anchor is still unreachable.', true)
    }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : String(e)
    flash(`Reconnect failed: ${msg}`, true)
  } finally {
    reconnecting.value = false
  }
}

// Poll instead of one-shot: a long-lived page (the kiosk especially) was
// stuck showing whatever anchor state existed at mount time forever.
//
// 02-11 gap closure: this card is rendered inside Server.vue, which joined
// KEEP_ALIVE_PATHS in 02-04 — but this poll was only ever armed in
// onMounted and disarmed in onUnmounted, so once Server.vue (and this card
// with it) is KeepAlive'd, onUnmounted never fires again and the poll ran
// forever in the background, firing `fips.status` every 15s regardless of
// which dashboard tab was actually showing (`document.hidden` only helps
// for a backgrounded BROWSER tab, not an in-SPA tab switch). Arm/disarm now
// follows activate/deactivate, mirroring Server.vue's own vpnPollInterval
// fix from 02-04; calling armFipsPoll from both onMounted and onActivated
// on a fresh KeepAlive-wrapped mount is safe since it always clears any
// prior timer first.
let statusInterval: ReturnType<typeof setInterval> | null = null
function armFipsPoll() {
  if (statusInterval) clearInterval(statusInterval)
  statusInterval = setInterval(() => {
    if (document.hidden) return
    void statusRes.refresh()
  }, 15000)
}
function disarmFipsPoll() {
  if (statusInterval) {
    clearInterval(statusInterval)
    statusInterval = null
  }
}
onMounted(() => {
  armFipsPoll()
})
onActivated(() => {
  armFipsPoll()
})
onDeactivated(() => {
  disarmFipsPoll()
})
onUnmounted(() => {
  disarmFipsPoll()
})
</script>
