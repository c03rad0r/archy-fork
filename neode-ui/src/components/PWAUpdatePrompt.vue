<template>
  <BaseModal :show="showUpdatePrompt" title="Update Available" z-index="z-[9999]" @close="dismissUpdate">
          <p class="text-white/80 mb-6">
            A new version of Archipelago is available. Update now to get the latest features and fixes.
          </p>
          <template #footer>
            <div class="flex gap-3 justify-end">
              <button
                @click="dismissUpdate"
                class="px-4 py-2 glass-button rounded-lg text-sm font-medium"
              >
                Later
              </button>
              <button
                @click="handleUpdate"
                class="px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium"
              >
                Update Now
              </button>
            </div>
          </template>
  </BaseModal>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import BaseModal from '@/components/BaseModal.vue'
import { useLoginTransitionStore } from '@/stores/loginTransition'
import { IS_DEMO } from '@/composables/useDemoIntro'

const showUpdatePrompt = ref(false)
let updateCallback: (() => Promise<void>) | null = null

// Reload for a genuine SW update — but never mid-cinematic. The splash
// typing intro and the dashboard-reveal cinematic are the two moments a
// surprise reload reads as "the app just reset itself" (kiosk and demo
// replay the intro on every boot/visit, so an update landing during it
// restarted the whole sequence). Wait until both are over, then reload.
function reloadAfterCinematic() {
  const loginTransition = useLoginTransitionStore()
  const calm = () =>
    document.body.classList.contains('splash-complete') &&
    !loginTransition.introCinematicPlaying
  if (calm()) {
    window.location.reload()
    return
  }
  const poll = setInterval(() => {
    if (calm()) {
      clearInterval(poll)
      window.location.reload()
    }
  }, 1000)
}

onMounted(() => {
  // The public demo has no version to update to — the prompt is noise, and
  // both accept-paths end in a reload that replays the demo intro ("the site
  // just reset itself"). skipWaiting/clientsClaim are off, so ignoring the
  // waiting worker is safe: this page keeps its complete old cache, and the
  // new build activates on the next visit.
  if (IS_DEMO) return
  // Listen for service worker updates
  if ('serviceWorker' in navigator) {
    // On the very first visit the page loads with no controlling SW; the
    // freshly-installed worker then activates and claims the page
    // (autoUpdate → clientsClaim), firing controllerchange. Reloading on
    // that first claim caused the fresh-install "loads, then reloads"
    // jank (worst on kiosk). Only reload when an existing controller is
    // REPLACED — i.e. a genuine update.
    let hadController = !!navigator.serviceWorker.controller
    navigator.serviceWorker.addEventListener('controllerchange', () => {
      if (!hadController) {
        hadController = true
        return
      }
      reloadAfterCinematic()
    })

    // Check for updates periodically
    const checkForUpdates = async () => {
      const registration = await navigator.serviceWorker.getRegistration()
      if (registration) {
        await registration.update()
      }
    }

    // Check for updates every 5 minutes
    setInterval(checkForUpdates, 5 * 60 * 1000)

    // Check when user returns to tab (helps with cached PWA)
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        checkForUpdates()
      }
    })

    // Listen for updatefound event
    navigator.serviceWorker.getRegistration().then((registration) => {
      if (registration) {
        registration.addEventListener('updatefound', () => {
          const newWorker = registration.installing
          if (newWorker) {
            newWorker.addEventListener('statechange', () => {
              if (newWorker.state === 'installed' && navigator.serviceWorker.controller) {
                updateCallback = async () => {
                  if (newWorker.state === 'installed' && registration.waiting) {
                    // Skip waiting and activate the new service worker
                    registration.waiting.postMessage({ type: 'SKIP_WAITING' })
                  }
                }
                // Auto-apply everywhere (alpha, user-approved 2026-07-31):
                // a prompt only reaches users who happen to click it, so
                // security fixes sat unapplied in long-lived sessions (the
                // installed PWA and kiosk displays especially). The kiosk
                // already auto-applied for exactly that reason; the rest of
                // the fleet now does too.
                //
                // Deliberately still routed through SKIP_WAITING rather than
                // build-time `skipWaiting: true`, so activation keeps the two
                // guards below: reloadAfterCinematic() holds the reload until
                // the splash/dashboard cinematic is over, and the
                // hadController check ignores the first-install claim. A
                // build-time skipWaiting would bypass both and could reload
                // the app out from under someone.
                //
                // Revisit at beta: restore the prompt (showUpdatePrompt.value
                // = true) if users should choose their own update moment.
                updateCallback()
              }
            })
          }
        })
      }
    })
  }
})

function dismissUpdate() {
  showUpdatePrompt.value = false
}

async function handleUpdate() {
  if (updateCallback) {
    await updateCallback()
  }
}
</script>
