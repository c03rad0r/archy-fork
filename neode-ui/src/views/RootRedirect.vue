<template>
  <div class="min-h-screen">
    <BootScreen :visible="showBootScreen" @ready="onServerReady" />
    <div v-if="!showBootScreen" class="min-h-screen flex items-center justify-center">
      <div class="flex flex-col items-center gap-4 opacity-0 root-redirect-fade">
        <svg class="animate-spin h-8 w-8 text-white/60" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { isOnboardingComplete } from '@/composables/useOnboarding'
import { IS_DEMO } from '@/composables/useDemoIntro'
import { isCompanionApp } from '@/utils/openExternal'
import BootScreen from '@/components/BootScreen.vue'

const router = useRouter()
const showBootScreen = ref(false)

/**
 * Public demo: every fresh boot of the app (first visit OR browser refresh at
 * the root) replays the intro for the full effect. In-session SPA navigation
 * never re-triggers it — this only runs when the app boots at '/'.
 */
function demoRoute() {
  log('demoRoute', { dest: '/onboarding/intro' })
  router.replace('/onboarding/intro').catch(() => {})
}

function log(msg: string, data?: unknown) {
  const ts = new Date().toISOString()
  const entry = `[RootRedirect ${ts}] ${msg}` + (data !== undefined ? ` ${JSON.stringify(data)}` : '')
  console.log(entry)
  const prev = sessionStorage.getItem('archipelago_boot_log') || ''
  sessionStorage.setItem('archipelago_boot_log', prev + entry + '\n')
}

async function quickHealthCheck(timeoutMs = 2000): Promise<boolean> {
  try {
    const ac = new AbortController()
    const t = setTimeout(() => ac.abort(), timeoutMs)
    const res = await fetch('/rpc/v1', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'server.echo', params: { message: 'ping' } }),
      signal: ac.signal,
    })
    clearTimeout(t)
    const ok = res.status !== 502 && res.status !== 503
    log('healthCheck', { status: res.status, ok })
    return ok
  } catch (e) {
    log('healthCheck FAILED', { error: String(e) })
    return false
  }
}

async function checkOnboarded(): Promise<boolean> {
  // No hard timeout here. isOnboardingComplete() already retries with
  // backoff (see useOnboarding.ts). A 3s Promise.race that resolves to
  // `false` on timeout was previously the main cause of the intro
  // flashing on already-onboarded nodes after browser-clear / reboot /
  // update: if the backend was slow to warm up, we'd force a 'false'
  // and route the user back through the setup wizard.
  try {
    const result = await isOnboardingComplete()
    log('checkOnboarded', { result })
    return result
  } catch (e) {
    const fallback = localStorage.getItem('neode_onboarding_complete') === '1'
    log('checkOnboarded ERROR, localStorage fallback', { error: String(e), fallback })
    return fallback
  }
}

async function proceedToApp() {
  if (IS_DEMO) {
    // Companion in-app demo skips the intro (acts as intro-already-seen) and
    // lands on /login; browser demo unaffected. No log() here — it writes
    // sessionStorage, and the skip path must write nothing.
    if (isCompanionApp()) {
      router.replace('/login').catch(() => {})
      return
    }
    demoRoute()
    return
  }
  const devMode = import.meta.env.VITE_DEV_MODE
  if (devMode === 'setup' || devMode === 'existing') {
    log('proceedToApp devMode', { devMode })
    router.replace('/login').catch(() => {})
    return
  }

  const onboarded = await checkOnboarded()
  const dest = onboarded ? '/login' : '/onboarding/intro'
  log('proceedToApp navigating', { onboarded, dest })
  router.replace(dest).catch(() => {})
}

function onServerReady() {
  if (import.meta.env.DEV) console.log('[RootRedirect] onServerReady — setting flag and reloading')
  localStorage.removeItem('neode_intro_seen')
  // Do NOT clear neode_onboarding_complete here — that flag must persist
  // across boot screen reloads so completed onboarding isn't lost.
  sessionStorage.setItem('archipelago_from_boot', '1')
  window.location.href = '/'
}

onMounted(async () => {
  const devMode = import.meta.env.VITE_DEV_MODE
  log('mounted', { devMode, from_boot: sessionStorage.getItem('archipelago_from_boot'), from_splash: sessionStorage.getItem('archipelago_from_splash') })

  if (sessionStorage.getItem('archipelago_from_boot') === '1') {
    log('from_boot=1, deferring to SplashScreen')
    return
  }

  if (sessionStorage.getItem('archipelago_from_splash') === '1') {
    log('from_splash=1, proceedToApp')
    proceedToApp()
    return
  }

  if (devMode === 'setup' || devMode === 'existing') {
    log('devMode shortcut', { devMode })
    proceedToApp()
    return
  }

  if (devMode === 'boot') {
    log('devMode=boot, showing boot screen')
    showBootScreen.value = true
    return
  }

  // First-boot backends can be up but slow (image loads, first-run podman
  // work), so a single 2s echo timing out used to flash the BootScreen and
  // then hard-reload seconds later ("loads, refreshes, loads again" on
  // kiosk). Give it a second, more patient attempt before concluding the
  // server is down.
  let isUp = await quickHealthCheck()
  if (!isUp) {
    log('healthCheck retry with longer timeout')
    isUp = await quickHealthCheck(6000)
  }
  log('production flow', { isUp })

  if (isUp) {
    // Demo: per-day intro gate instead of server-side onboarding state.
    if (IS_DEMO) {
      // Companion in-app demo skips the intro; browser demo unaffected.
      // No log() — the skip path must write nothing to storage.
      if (isCompanionApp()) {
        router.replace('/login').catch(() => {})
        return
      }
      demoRoute()
      return
    }
    const onboarded = await checkOnboarded()
    if (onboarded) {
      log('server up + onboarded → proceedToApp')
      proceedToApp()
      return
    }
    log('server up + NOT onboarded → onboarding intro')
    router.replace('/onboarding/intro').catch(() => {})
    return
  }

  // Server not ready. The full BootScreen is meant for a genuine
  // cold-start (fresh install), not for the brief blip during an
  // OTA update where the backend restarts. If onboarding has already
  // completed we just keep the spinner and retry until the server
  // responds again.
  const wasOnboardedBefore = localStorage.getItem('neode_onboarding_complete') === '1'
  if (wasOnboardedBefore) {
    log('server down + onboarded → polling without boot screen')
    let retries = 0
    const maxRetries = 30 // 30 * 2s = 60s before giving up and showing boot screen
    const poll = setInterval(async () => {
      retries++
      if (await quickHealthCheck()) {
        clearInterval(poll)
        proceedToApp()
        return
      }
      if (retries >= maxRetries) {
        clearInterval(poll)
        log('server still down after retries → falling back to boot screen')
        showBootScreen.value = true
      }
    }, 2000)
    return
  }
  showBootScreen.value = true
})
</script>

<style scoped>
.root-redirect-fade {
  animation: root-fade-in 0.3s ease 0.5s forwards;
}
@keyframes root-fade-in {
  to { opacity: 1; }
}
</style>
