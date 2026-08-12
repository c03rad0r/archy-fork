<template>
  <Teleport to="body">
    <!-- Lifecycle / Offline Banner.
         Server restart/shutdown is deliberate → shown immediately. A plain
         connection blip is debounced (showConnIssue) so transient sub-grace
         reconnects don't flash. -->
    <Transition name="conn-banner">
      <div
        v-if="(showLifecycle || showConnectionLost)"
        class="conn-banner-overlay"
      >
        <div class="path-option-card px-6 py-3 border-l-4 border-yellow-500 inline-flex items-center gap-2 text-yellow-200 shadow-2xl">
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
          </svg>
          <span class="font-medium">
            {{ isRestarting ? 'Server is restarting...' : isShuttingDown ? 'Server is shutting down...' : 'Connection lost' }}
          </span>
        </div>
      </div>
    </Transition>

    <!-- Reconnecting Banner (debounced) -->
    <Transition name="conn-banner">
      <div
        v-if="showReconnecting"
        class="conn-banner-overlay"
      >
        <div class="path-option-card px-6 py-3 border-l-4 border-blue-500 inline-flex items-center gap-2 text-blue-200 shadow-2xl">
          <svg class="w-5 h-5 animate-spin" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
          </svg>
          <span class="font-medium">Reconnecting...</span>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from 'vue'
import { useAppStore } from '@/stores/app'
import { IS_DEMO } from '@/composables/useDemoIntro'

const store = useAppStore()

const isOffline = computed(() => store.isOffline)
const isRestarting = computed(() => store.isRestarting)
const isShuttingDown = computed(() => store.isShuttingDown)

// A deliberate server lifecycle transition (restart/shutdown) is real and
// user-initiated — surface it immediately, no debounce.
const isLifecycleTransition = computed(() => isRestarting.value || isShuttingDown.value)
const showLifecycle = computed(() => isLifecycleTransition.value && store.isAuthenticated)

// A plain connection blip (offline or reconnecting, not a lifecycle transition).
// The overwhelming majority recover within a second or two (load spikes,
// Tailscale/relay TCP resets), so showing the banner instantly makes a healthy
// node read as unstable. Debounce: only surface after the issue persists past a
// grace window; hide immediately on recovery.
const hasConnIssue = computed(
  () => (store.isReconnecting || isOffline.value) && !isLifecycleTransition.value
)

const SHOW_DELAY_MS = 2500
// Right after the page loads or the tab returns to the foreground, a dead
// WebSocket is the NORMAL state (browsers kill sockets in background tabs;
// first paint races the initial connect). Reconnecting takes longer than
// the steady-state grace on real links — radio wake-up, TLS, proxies — so
// the 2.5s window made every tab-return flash "Connection lost" on a
// perfectly healthy node. Give those moments a much longer runway; keep
// the short window for genuine mid-session drops.
const RESUME_GRACE_WINDOW_MS = 15000
const RESUME_SHOW_DELAY_MS = 10000
const showConnIssue = ref(false)
let pendingTimer: ReturnType<typeof setTimeout> | null = null
let lastResumeAt = Date.now() // mount counts as a resume (initial connect)

function onVisibilityResume() {
  if (!document.hidden) lastResumeAt = Date.now()
}
document.addEventListener('visibilitychange', onVisibilityResume)

function clearTimer() {
  if (pendingTimer) {
    clearTimeout(pendingTimer)
    pendingTimer = null
  }
}

watch(
  hasConnIssue,
  (issue) => {
    clearTimer()
    // The demo runs against a local mock — a connection banner there is
    // meaningless noise on what should be a flawless showcase.
    if (IS_DEMO) return
    if (issue) {
      const delay = Date.now() - lastResumeAt < RESUME_GRACE_WINDOW_MS
        ? RESUME_SHOW_DELAY_MS
        : SHOW_DELAY_MS
      pendingTimer = setTimeout(() => {
        showConnIssue.value = true
        pendingTimer = null
      }, delay)
    } else {
      // Recovered before the grace window elapsed — hide at once.
      showConnIssue.value = false
    }
  },
  { immediate: true }
)

onUnmounted(() => {
  clearTimer()
  document.removeEventListener('visibilitychange', onVisibilityResume)
})

// Debounced visual states the template renders.
const showReconnecting = computed(
  () => showConnIssue.value && store.isReconnecting && store.isAuthenticated
)
const showConnectionLost = computed(
  () => showConnIssue.value && isOffline.value && !store.isReconnecting && store.isAuthenticated
)
</script>

<style scoped>
/* Float the connection banners over the UI instead of occupying layout space
 * (which previously pushed the whole dashboard down when reconnecting).
 * Pinned top-center, clear of the status bar via the safe-area inset that the
 * Android companion app injects (--safe-area-top), falling back to env(). */
.conn-banner-overlay {
  position: fixed;
  top: calc(1rem + var(--safe-area-top, env(safe-area-inset-top, 0px)));
  left: 50%;
  z-index: 60;
  transform: translateX(-50%);
  max-width: calc(100% - 2rem);
  pointer-events: none; /* purely informational — never intercept taps */
}

.conn-banner-enter-active,
.conn-banner-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}
.conn-banner-enter-from,
.conn-banner-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-8px);
}
.conn-banner-enter-to,
.conn-banner-leave-from {
  opacity: 1;
  transform: translateX(-50%) translateY(0);
}
</style>
