<script setup lang="ts">
import { ref, computed, watch, onBeforeUnmount } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { useAppStore } from '@/stores/app'

// Global "back up your Lightning seed" nudge. The full backup flow lives on
// the LND app-details page (views/appDetails/LndSeedBackup.vue); this banner
// just surfaces it proactively once a wallet seed exists and hasn't been
// acknowledged as backed up yet.
//
// lnd.seed-backup-status → { available, acknowledged }:
//   available=false    → no captured seed (LND not installed / legacy wallet) → stay quiet
//   acknowledged=true  → user already confirmed the backup → stay quiet
//   available && !acknowledged → prompt

const SNOOZE_KEY = 'lnd-seed-backup-prompt-snooze-until'
const SNOOZE_MS = 24 * 60 * 60 * 1000
const POLL_MS = 5 * 60 * 1000
const LND_DETAILS_PATH = '/dashboard/apps/lnd'

const router = useRouter()
const route = useRoute()
const appStore = useAppStore()

const needsBackup = ref(false)
const snoozedUntil = ref(readSnooze())
let pollTimer: ReturnType<typeof setInterval> | null = null
// Once the backend says acked (or there is no seed to back up), stop polling
// for the rest of the session — nothing left for this banner to do.
let settled = false

function readSnooze(): number {
  try {
    const raw = localStorage.getItem(SNOOZE_KEY)
    const ts = raw ? Number(raw) : 0
    return Number.isFinite(ts) ? ts : 0
  } catch {
    return 0
  }
}

function isAuthed(): boolean {
  // Same guard as useMessageToast — never poll unauthenticated (background
  // 401s on the login page cause refresh loops).
  try { return localStorage.getItem('neode-auth') === 'true' } catch { return false }
}

async function pollStatus() {
  if (settled || !isAuthed()) return
  try {
    const res = await rpcClient.call<{ available: boolean; acknowledged: boolean }>({
      method: 'lnd.seed-backup-status',
      timeout: 5000,
    })
    if (!res.available || res.acknowledged) {
      needsBackup.value = false
      settled = true
      stopPolling()
    } else {
      needsBackup.value = true
    }
  } catch {
    // Transient RPC failure — keep the current state, retry on next tick.
  }
}

function startPolling() {
  if (pollTimer || settled) return
  void pollStatus()
  pollTimer = setInterval(() => {
    if (document.visibilityState === 'visible') void pollStatus()
  }, POLL_MS)
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer)
    pollTimer = null
  }
}

watch(() => appStore.isAuthenticated, (authed) => {
  if (authed) startPolling()
  else {
    stopPolling()
    needsBackup.value = false
  }
}, { immediate: true })

// The LND details page renders its own prominent backup card — hide the
// banner there, and re-check status when the user navigates away so an ack
// made on that page hides the banner immediately (not after the next tick).
const onLndPage = computed(() => route.path === LND_DETAILS_PATH)
watch(onLndPage, (now, was) => {
  if (was && !now && !settled && isAuthed()) void pollStatus()
})

const show = computed(() =>
  needsBackup.value && !onLndPage.value && Date.now() >= snoozedUntil.value,
)

function backUpNow() {
  // seed-backup=1 makes the LND page open the reveal flow immediately —
  // landing on the app page with the card below the fold read as
  // "clicking the notification did nothing".
  router.push({ path: LND_DETAILS_PATH, query: { 'seed-backup': '1' } }).catch(() => {})
}

function snooze() {
  const until = Date.now() + SNOOZE_MS
  snoozedUntil.value = until
  try { localStorage.setItem(SNOOZE_KEY, String(until)) } catch { /* noop */ }
}

onBeforeUnmount(stopPolling)
</script>

<template>
  <Teleport to="body">
    <Transition name="seed-banner">
      <div
        v-if="show"
        role="alert"
        class="fixed bottom-4 left-4 right-4 z-[95] mx-auto w-auto max-w-lg rounded-xl border border-orange-400/30 p-4 seed-banner-glass md:left-auto md:right-6 md:bottom-6"
      >
        <div class="flex items-start gap-3">
          <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-orange-500/20">
            <svg class="h-5 w-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M10.29 3.86l-8.4 14.55A1.5 1.5 0 003.19 21h17.62a1.5 1.5 0 001.3-2.59l-8.4-14.55a1.5 1.5 0 00-2.62 0z" />
            </svg>
          </div>
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium text-white">Back up your Lightning wallet seed</p>
            <p class="mt-0.5 text-sm text-white/70">
              Your Lightning wallet has a recovery seed that hasn't been backed up yet.
              Write it down now so your funds survive a disk failure.
            </p>
            <div class="mt-3 flex gap-2">
              <button
                type="button"
                class="glass-button glass-button-warning rounded-lg px-4 py-2 text-sm font-medium"
                @click="backUpNow"
              >Back up now</button>
              <button
                type="button"
                class="glass-button rounded-lg px-4 py-2 text-sm font-medium"
                @click="snooze"
              >Later</button>
            </div>
          </div>
          <button
            type="button"
            aria-label="Dismiss for now"
            class="-mt-1 -mr-1 shrink-0 rounded-full p-1 text-white/40 transition-colors hover:bg-white/10 hover:text-white/80"
            @click="snooze"
          >
            <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.seed-banner-glass {
  background: rgba(15, 15, 20, 0.85);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.45);
}

.seed-banner-enter-active,
.seed-banner-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}

.seed-banner-enter-from,
.seed-banner-leave-to {
  opacity: 0;
  transform: translateY(12px);
}
</style>
