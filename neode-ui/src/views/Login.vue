<template>
  <div class="min-h-screen flex items-center justify-center p-4 relative z-10 login-fly-perspective">
    <div class="w-full max-w-md relative z-20">
      <!-- Login Card - flies towards user on success -->
      <div
        class="glass-card p-8 pt-20 relative login-card overflow-visible"
        :class="{ 'login-fly-towards': whooshAway }"
      >
        <!-- Logo - half in, half out of container -->
        <div class="absolute -top-10 left-1/2 -translate-x-1/2 z-10">
          <div class="logo-gradient-border logo-gloss w-20 h-20">
            <AnimatedLogo no-border fit />
          </div>
        </div>

        <!-- Title -->
        <h1 class="text-2xl font-semibold text-white/96 text-center mb-8 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          <span v-if="isCheckingSetup">&nbsp;</span>
          <span v-else-if="isSetupMode && !isSetup">{{ t('login.setupTitle') }}</span>
          <span v-else>{{ t('login.title') }}</span>
        </h1>

        <!-- Server Startup Progress -->
        <div v-if="!serverReady" class="mb-6" role="status" aria-live="polite">
          <div class="flex items-center justify-center gap-2 mb-3">
            <svg class="animate-spin h-4 w-4 text-orange-400" aria-hidden="true" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            <span class="text-sm text-white/60">{{ t('login.serverStarting') }}</span>
          </div>
          <div class="startup-progress-track">
            <div class="startup-progress-bar" :style="{ width: startupProgress + '%' }"></div>
          </div>
        </div>

        <!-- Error Message -->
        <div v-if="error" role="alert" class="mb-4 p-3 bg-red-500/20 border border-red-500/40 rounded-lg text-red-200 text-sm">
          {{ error }}
        </div>

        <!-- Checking setup state -->
        <div v-if="isCheckingSetup" class="flex items-center justify-center py-8">
          <svg class="animate-spin h-6 w-6 text-white/40" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
          </svg>
        </div>

        <!-- Setup Mode: Password Setup -->
        <template v-else-if="isSetupMode && !isSetup">
          <div class="mb-4 p-4 bg-white/5 border border-white/10 rounded-lg text-white/80 text-sm">
            <p class="mb-2">Create a password to secure your Archipelago node.</p>
            <p class="text-white/60 text-xs">This password will be required to access your node.</p>
          </div>

          <div class="mb-4">
            <label for="setup-password" class="block text-sm font-medium text-white/80 mb-2">
              {{ t('login.password') }}
            </label>
            <input
              id="setup-password"
              v-model="password"
              type="password"
              autocomplete="new-password"
              data-form-type="other"
              data-controller-no-submit
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/40 focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
              :placeholder="t('login.enterPasswordSetup')"
              @keydown.enter="confirmPasswordInputRef?.focus()"
              @input="error = null"
              :disabled="loading || formDisabled"
            />
          </div>

          <div class="mb-6">
            <label for="setup-confirm-password" class="block text-sm font-medium text-white/80 mb-2">
              {{ t('login.confirmPassword') }}
            </label>
            <input
              id="setup-confirm-password"
              ref="confirmPasswordInputRef"
              v-model="confirmPassword"
              type="password"
              autocomplete="new-password"
              data-form-type="other"
              data-controller-no-submit
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/40 focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
              :placeholder="t('login.confirmPasswordPlaceholder')"
              @keydown.enter="handleSetupWithSound"
              @input="error = null"
              :disabled="loading || formDisabled"
            />
          </div>

          <button
            @click="handleSetupWithSound"
            :disabled="loading || formDisabled"
            class="w-full glass-button px-6 py-3 rounded-lg font-medium transition-all hover:bg-black/70 hover:border-white/30 disabled:opacity-50 disabled:cursor-not-allowed"
            :class="{ 'opacity-60': !password || password.length < 8 || password !== confirmPassword }"
          >
            <span v-if="!loading">{{ t('login.setupButton') }}</span>
            <span v-else class="flex items-center justify-center">
              <svg class="animate-spin h-5 w-5 mr-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              {{ t('login.settingUp') }}
            </span>
          </button>
        </template>

        <!-- TOTP Verification Step -->
        <template v-else-if="requiresTotp">
          <div class="mb-6 text-center">
            <svg xmlns="http://www.w3.org/2000/svg" class="w-12 h-12 mx-auto mb-3 text-orange-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z" />
            </svg>
            <p class="text-white/80 text-sm mb-1">{{ t('login.twoFactorTitle') }}</p>
            <p class="text-white/50 text-xs">{{ t('login.totpInstruction') }}</p>
          </div>

          <div class="mb-4">
            <input
              ref="totpInputRef"
              v-model="totpCode"
              type="text"
              inputmode="numeric"
              pattern="[0-9]*"
              maxlength="8"
              autocomplete="one-time-code"
              data-controller-no-submit
              :aria-label="t('login.totpLabel')"
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white text-center text-2xl tracking-[0.5em] placeholder-white/40 focus:outline-none focus:border-orange-400/60 focus:ring-1 focus:ring-orange-400/30 transition-colors"
              :placeholder="useBackupCode ? 'XXXX-XXXX' : '000000'"
              @keyup.enter="handleTotpVerify"
              :disabled="loading"
            />
          </div>

          <button
            @click="handleTotpVerify"
            :disabled="loading || !totpCode"
            class="w-full glass-button px-6 py-3 rounded-lg font-medium transition-all hover:bg-black/70 hover:border-white/30 disabled:opacity-50 disabled:cursor-not-allowed mb-3"
          >
            <span v-if="!loading">{{ t('login.verifyButton') }}</span>
            <span v-else class="flex items-center justify-center">
              <svg class="animate-spin h-5 w-5 mr-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              {{ t('login.verifying') }}
            </span>
          </button>

          <button
            @click="useBackupCode = !useBackupCode; totpCode = ''"
            class="w-full text-white/50 text-sm hover:text-white/70 transition-colors py-2"
          >
            {{ useBackupCode ? t('login.useAuthCode') : t('login.useBackupCode') }}
          </button>
        </template>

        <!-- Normal Login Mode -->
        <template v-else>
          <!-- Demo credential hint -->
          <div v-if="isDemo" class="mb-4 p-3 bg-orange-500/15 border border-orange-400/30 rounded-lg text-orange-100 text-sm text-center">
            🎮 Demo mode — Password: <span class="font-mono font-semibold">{{ DEMO_PASSWORD }}</span>
          </div>

          <!-- All auth inputs opt out of controller-nav's Enter→click-next-button
               pattern (data-controller-no-submit): they submit via their own Enter
               handlers, and while the submit button is still disabled the "next
               focusable" is Replay Intro — the companion's auto-login injects
               Enter before Vue re-enables the button, which replayed the intro
               in a loop on every app connect. -->
          <div class="mb-6">
            <label for="login-password" class="block text-sm font-medium text-white/80 mb-2">
              {{ t('login.password') }}
            </label>
            <input
              id="login-password"
              v-model="password"
              type="password"
              autocomplete="current-password"
              data-form-type="other"
              data-controller-no-submit
              class="w-full px-4 py-3 bg-transparent border border-white/20 rounded-lg text-white placeholder-white/40 focus:outline-none focus:border-white/40 focus:ring-1 focus:ring-white/20 transition-colors"
              :placeholder="t('login.enterPasswordPlaceholder')"
              @keydown.enter="handleLoginWithSound"
              :disabled="loading || formDisabled"
            />
          </div>

          <button
            @click="handleLoginWithSound"
            :disabled="loading || formDisabled || !password"
            class="w-full glass-button px-6 py-3 rounded-lg font-medium transition-all hover:bg-black/70 hover:border-white/30 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <span v-if="!loading">{{ t('login.loginButton') }}</span>
            <span v-else class="flex items-center justify-center">
              <svg class="animate-spin h-5 w-5 mr-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              {{ t('login.loggingIn') }}
            </span>
          </button>
        </template>

        <!-- Footer Links -->
        <div class="mt-6 text-center text-sm text-white/40">
          {{ t('login.recoveryNote') }}
        </div>
      </div>

      <!-- Replay Intro / Restart Onboarding - Bottom of Page -->
      <div class="mt-8 text-center flex items-center justify-center gap-4">
        <button 
          @click="replayIntro" 
          class="text-xs text-white/50 hover:text-white/70 transition-colors underline-offset-2 hover:underline"
        >
          {{ t('login.replayIntro') }}
        </button>
        <template v-if="!isDemo">
          <span class="text-white/30">|</span>
          <button
            @click="restartOnboarding"
            :disabled="isResettingOnboarding"
            class="text-xs transition-colors underline-offset-2 hover:underline disabled:opacity-50 disabled:cursor-not-allowed"
            :class="confirmingRestartOnboarding ? 'text-orange-400 hover:text-orange-300' : 'text-white/50 hover:text-white/70'"
          >
            {{ isResettingOnboarding ? t('login.resetting') : (confirmingRestartOnboarding ? t('login.restartConfirm') : t('login.onboarding')) }}
          </button>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { isLocalRedirect } from '../router'
import { useI18n } from 'vue-i18n'
import AnimatedLogo from '@/components/AnimatedLogo.vue'
import { useAppStore } from '../stores/app'

const { t } = useI18n()
import { useLoginTransitionStore } from '../stores/loginTransition'
import { rpcClient } from '../api/rpc-client'
import { resumeAudioContext, startSynthwave, stopSynthwave, playLoginSuccessWhoosh, playPop } from '@/composables/useLoginSounds'
import { IS_DEMO, DEMO_PASSWORD, clearDemoIntroSeen } from '@/composables/useDemoIntro'

const router = useRouter()
const currentRoute = useRoute()

/** After login, redirect to the intended page or default to home */
const loginRedirectTo = computed(() => {
  const redirect = currentRoute.query.redirect as string
  if (redirect && isLocalRedirect(redirect)) return redirect
  return '/dashboard'
})
const store = useAppStore()
const loginTransition = useLoginTransitionStore()

const isDemo = IS_DEMO
const password = ref(IS_DEMO ? DEMO_PASSWORD : '')
const confirmPassword = ref('')
const loading = ref(false)
const error = ref<string | null>(null)
const isSetup = ref(false)
const whooshAway = ref(false)
const requiresTotp = ref(false)
const totpCode = ref('')
const useBackupCode = ref(false)
const totpInputRef = ref<HTMLInputElement | null>(null)
const confirmPasswordInputRef = ref<HTMLInputElement | null>(null)

// Server startup state
const serverReady = ref(false)
const serverChecking = ref(true)
const startupProgress = ref(0)
let startupPollTimer: ReturnType<typeof setTimeout> | null = null
let startupProgressInterval: ReturnType<typeof setInterval> | null = null

// Whether we're in setup mode (no password created yet)
const isSetupMode = ref(false)

// Whether we're still checking the setup state (prevents flash of wrong form)
const isCheckingSetup = ref(true)

// Whether the login form should be disabled (server not ready)
const formDisabled = computed(() => !serverReady.value)

async function checkServerHealth(): Promise<boolean> {
  try {
    const response = await fetch('/rpc/v1', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ method: 'server.echo', params: { message: 'ping' } }),
      signal: AbortSignal.timeout(5000),
    })
    // Any HTTP response from backend (200, 401, 403, etc.) means it's up
    // Only 502/503 from nginx means backend isn't running yet
    return response.status !== 502 && response.status !== 503
  } catch {
    return false
  }
}

function pollServerStartup(): Promise<void> {
  return new Promise((resolve) => {
    // Animate progress slowly while waiting
    startupProgressInterval = setInterval(() => {
      if (startupProgress.value < 90) {
        startupProgress.value += Math.random() * 8 + 2
        if (startupProgress.value > 90) startupProgress.value = 90
      }
    }, 600)

    const poll = async () => {
      const healthy = await checkServerHealth()
      if (healthy) {
        if (startupProgressInterval) clearInterval(startupProgressInterval)
        startupProgress.value = 100
        // Brief pause to show 100% before revealing form
        await new Promise(r => setTimeout(r, 400))
        serverReady.value = true
        serverChecking.value = false
        resolve()
        return
      }
      // Retry in 2s
      startupPollTimer = setTimeout(poll, 2000)
    }

    poll()
  })
}

let unlockHandler: (() => void) | null = null

function removeUnlockListeners() {
  if (unlockHandler) {
    document.removeEventListener('click', unlockHandler)
    document.removeEventListener('touchstart', unlockHandler)
    document.removeEventListener('keydown', unlockHandler)
    unlockHandler = null
  }
}

onBeforeUnmount(() => {
  removeUnlockListeners()
  if (startupPollTimer) clearTimeout(startupPollTimer)
  if (startupProgressInterval) clearInterval(startupProgressInterval)
  if (confirmRestartTimer) clearTimeout(confirmRestartTimer)
})

onMounted(async () => {
  const fromSplash = sessionStorage.getItem('archipelago_from_splash') === '1'
  if (fromSplash) sessionStorage.removeItem('archipelago_from_splash')
  unlockHandler = () => {
    if (!fromSplash) {
      resumeAudioContext()
      startSynthwave()
    }
    removeUnlockListeners()
  }
  document.addEventListener('click', unlockHandler, { once: true })
  document.addEventListener('touchstart', unlockHandler, { once: true })
  document.addEventListener('keydown', unlockHandler, { once: true })

  // Check server health first
  const healthy = await checkServerHealth()
  if (healthy) {
    serverReady.value = true
    serverChecking.value = false
  } else {
    // Server not ready — start polling with progress bar
    await pollServerStartup()
  }

  // Check if password has been set up — show setup form if not
  try {
    const result = await rpcClient.call<boolean>({ method: 'auth.isSetup', params: {}, timeout: 8000 })
    isSetup.value = Boolean(result)
    isSetupMode.value = !isSetup.value
  } catch {
    isSetup.value = false
    isSetupMode.value = true
  } finally {
    isCheckingSetup.value = false
  }
})


function handleSetupWithSound() {
  if (!loading.value && password.value && password.value === confirmPassword.value) {
    playPop()
  }
  handleSetup()
}

async function handleSetup() {
  if (!password.value || password.value.length < 8) {
    error.value = t('login.errorMinLength')
    return
  }

  if (password.value !== confirmPassword.value) {
    error.value = t('login.errorMismatch')
    return
  }

  loading.value = true
  error.value = null

  try {
    await rpcClient.call({ 
      method: 'auth.setup', 
      params: { password: password.value.trim() }
    })

    await store.login(password.value.trim())
    // Verify session cookie works before navigating (prevents connection lost on first login)
    try {
      await rpcClient.call({ method: 'server.echo', params: { message: 'session-check' } })
    } catch {
      error.value = 'Setup succeeded but session could not be established. Try refreshing.'
      store.logout()
      return
    }
    stopSynthwave()
    whooshAway.value = true
    playLoginSuccessWhoosh()
    loginTransition.setJustCompletedOnboarding(true)
    loginTransition.setJustLoggedIn(true)
    // First password setup counts as the first login (video → static rotation).
    try { localStorage.setItem('neode_first_login_done', '1') } catch { /* ignore */ }
    await new Promise(r => setTimeout(r, 520))
    await router.replace(loginRedirectTo.value).catch(() => {
      window.location.href = loginRedirectTo.value
    })
  } catch (err) {
    whooshAway.value = false
    const msg = err instanceof Error ? err.message : ''
    if (/502|503|Bad Gateway|timeout|fetch|network/i.test(msg)) {
      error.value = t('login.errorServerStarting')
    } else {
      error.value = msg || t('login.errorSetupFailed')
    }
    startSynthwave()
  } finally {
    loading.value = false
  }
}

function handleLoginWithSound() {
  if (!loading.value && password.value) {
    playPop()
  }
  handleLogin()
}

async function handleLogin() {
  if (!password.value) return

  loading.value = true
  error.value = null

  try {
    const result = await store.login(password.value.trim())
    if (result?.requires_totp) {
      requiresTotp.value = true
      loading.value = false
      // Focus the TOTP input after DOM update
      setTimeout(() => totpInputRef.value?.focus(), 100)
      return
    }
    // Verify session cookie works before navigating (prevents login loop on LAN)
    try {
      await rpcClient.call({ method: 'server.echo', params: { message: 'session-check' } })
    } catch {
      error.value = 'Login succeeded but session could not be established. Try clearing cookies and refreshing.'
      store.logout()
      return
    }
    stopSynthwave()
    whooshAway.value = true
    playLoginSuccessWhoosh()
    consumeOnboardingFinale()
    loginTransition.setJustLoggedIn(true)
    await new Promise(r => setTimeout(r, 520))
    await router.replace(loginRedirectTo.value).catch(() => {
      window.location.href = loginRedirectTo.value
    })
  } catch (err) {
    whooshAway.value = false
    const msg = err instanceof Error ? err.message : ''
    if (/502|503|Bad Gateway|timeout|fetch|network/i.test(msg)) {
      error.value = t('login.errorServerStarting')
    } else {
      error.value = msg || t('login.errorLoginFailed')
    }
    startSynthwave()
  } finally {
    loading.value = false
  }
}

async function handleTotpVerify() {
  if (!totpCode.value) return

  loading.value = true
  error.value = null

  try {
    if (useBackupCode.value) {
      await rpcClient.loginBackup(totpCode.value)
    } else {
      await rpcClient.loginTotp(totpCode.value)
    }
    await store.completeLoginAfterTotp()
    stopSynthwave()
    whooshAway.value = true
    playLoginSuccessWhoosh()
    consumeOnboardingFinale()
    loginTransition.setJustLoggedIn(true)
    await new Promise(r => setTimeout(r, 520))
    await router.replace(loginRedirectTo.value).catch(() => {
      window.location.href = loginRedirectTo.value
    })
  } catch (err) {
    const msg = err instanceof Error ? err.message : ''
    if (/expired|too many/i.test(msg)) {
      // Session expired, go back to password step
      requiresTotp.value = false
      totpCode.value = ''
      error.value = msg
    } else {
      error.value = msg || t('login.errorInvalidCode')
    }
    totpCode.value = ''
  } finally {
    loading.value = false
  }
}

/** A login right after the onboarding wizard (flag set by OnboardingDone)
 *  gets the FULL dashboard entrance — zoom + oomph — even though it's a
 *  regular password login (e.g. the demo). Subsequent logins stay
 *  deliberately low-key (justLoggedIn only).
 *  Also stamps neode_first_login_done: the login page keeps the intro VIDEO
 *  background until someone has logged in once (OnboardingWrapper reads it
 *  to pick video vs the rotating static backgrounds). */
function consumeOnboardingFinale() {
  try {
    if (sessionStorage.getItem('archy_onboarding_finale') === '1') {
      sessionStorage.removeItem('archy_onboarding_finale')
      loginTransition.setJustCompletedOnboarding(true)
    }
    localStorage.setItem('neode_first_login_done', '1')
  } catch { /* ignore */ }
}

function replayIntro() {
  // Clear the intro seen flag
  localStorage.removeItem('neode_intro_seen')
  // Demo: also clear the per-day gate so the intro plays again now.
  if (IS_DEMO) clearDemoIntroSeen()
  // On an onboarded node App.vue instantly re-marks the intro as seen (that's
  // what keeps fresh browsers on already-onboarded nodes from replaying it) —
  // this explicit one-shot flag tells it the replay is deliberate.
  try { sessionStorage.setItem('archipelago_replay_intro', '1') } catch { /* ignore */ }
  // Navigate to root to trigger splash screen
  window.location.href = '/'
}

const isResettingOnboarding = ref(false)
const confirmingRestartOnboarding = ref(false)
let confirmRestartTimer: ReturnType<typeof setTimeout> | null = null

async function restartOnboarding() {
  if (isResettingOnboarding.value) return
  // First click arms a confirmation state; only a second explicit click restarts.
  if (!confirmingRestartOnboarding.value) {
    confirmingRestartOnboarding.value = true
    if (confirmRestartTimer) clearTimeout(confirmRestartTimer)
    confirmRestartTimer = setTimeout(() => {
      confirmingRestartOnboarding.value = false
      confirmRestartTimer = null
    }, 5000)
    return
  }
  if (confirmRestartTimer) {
    clearTimeout(confirmRestartTimer)
    confirmRestartTimer = null
  }
  confirmingRestartOnboarding.value = false
  isResettingOnboarding.value = true
  // Local-only reset — no RPC needed since user isn't logged in.
  // Onboarding pages are all public, so clearing localStorage is enough.
  localStorage.removeItem('neode_onboarding_complete')
  localStorage.removeItem('neode_did')
  localStorage.removeItem('neode_did_state')
  localStorage.removeItem('neode_backup_created')
  router.push('/onboarding/intro').then(() => {
    window.location.reload()
  }).catch(() => {
    window.location.href = '/onboarding/intro'
  })
}
</script>

<style scoped>
/* Server startup progress bar */
.startup-progress-track {
  height: 4px;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 2px;
  overflow: hidden;
}

.startup-progress-bar {
  height: 100%;
  background: linear-gradient(90deg, #fb923c, #f59e0b);
  border-radius: 2px;
  transition: width 0.5s ease-out;
  box-shadow: 0 0 8px rgba(251, 146, 60, 0.4);
}

/* Perspective for 3D fly effect */
.login-fly-perspective {
  perspective: 1200px;
  perspective-origin: center center;
}

.login-card {
  overflow: visible !important;
  transform-style: preserve-3d;
  transition: transform 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94),
              opacity 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94),
              filter 0.5s ease-out;
}

/* Fly towards user - card zooms forward as it transitions out */
.login-fly-towards {
  animation: login-fly-towards 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94) forwards;
}

@keyframes login-fly-towards {
  0% {
    transform: translateZ(0) scale(1);
    opacity: 1;
    filter: blur(0);
  }
  60% {
    transform: translateZ(180px) scale(1.4);
    opacity: 0.95;
    filter: blur(2px);
  }
  100% {
    transform: translateZ(400px) scale(2);
    opacity: 0;
    filter: blur(8px);
  }
}
</style>
