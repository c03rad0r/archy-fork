<script setup lang="ts">
import { ref, computed, onBeforeUnmount } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { rpcClient } from '@/api/rpc-client'
import ScreensaverRing from '@/components/ScreensaverRing.vue'
import ScreensaverLogo from '@/components/ScreensaverLogo.vue'

const router = useRouter()
const { t } = useI18n()

// Reboot
const showRebootConfirm = ref(false)
const rebooting = ref(false)
const rebootPassword = ref('')
const rebootError = ref('')

// Reboot overlay — full-screen progress shown once the reboot is committed.
// Mirrors the update overlay pattern in SystemUpdate.vue: poll /health,
// auto-reload when the backend returns, stall fallback at 3 min.
type RebootStage = 'rebooting' | 'reconnecting' | 'ready' | 'stalled'
const rebootOverlay = ref(false)
const rebootStage = ref<RebootStage>('rebooting')
const rebootStartedAt = ref(0)
const rebootElapsedSec = ref(0)
let rebootPollTimer: ReturnType<typeof setInterval> | null = null
let rebootElapsedTimer: ReturnType<typeof setInterval> | null = null
const rebootElapsedLabel = computed(() => {
  const s = rebootElapsedSec.value
  if (s < 60) return `Elapsed: ${s}s`
  return `Elapsed: ${Math.floor(s / 60)}m${s % 60 < 10 ? '0' : ''}${s % 60}s`
})

function startRebootOverlay() {
  rebootOverlay.value = true
  rebootStage.value = 'rebooting'
  rebootStartedAt.value = Date.now()
  rebootElapsedSec.value = 0
  rebootElapsedTimer = setInterval(() => {
    rebootElapsedSec.value = Math.floor((Date.now() - rebootStartedAt.value) / 1000)
    if (rebootElapsedSec.value >= 180 && rebootStage.value !== 'ready') {
      rebootStage.value = 'stalled'
    }
  }, 1000)
  // Start health polling after 2.5s — the kernel has to go down before
  // /health can disappear, and we don't want to see the pre-reboot health
  // reply and mis-report "ready".
  setTimeout(() => {
    rebootStage.value = 'reconnecting'
    rebootPollTimer = setInterval(pollRebootHealth, 1500)
  }, 2500)
}
async function pollRebootHealth() {
  if (rebootStage.value === 'ready' || rebootStage.value === 'stalled') return
  try {
    const res = await fetch('/health', { signal: AbortSignal.timeout(2000) })
    if (!res.ok) throw new Error(`health ${res.status}`)
    rebootStage.value = 'ready'
    if (rebootPollTimer) { clearInterval(rebootPollTimer); rebootPollTimer = null }
    setTimeout(() => { window.location.reload() }, 1200)
  } catch {
    // Fetch failing is the normal state while the host is down.
  }
}
function rebootReloadNow() { window.location.reload() }
onBeforeUnmount(() => {
  if (rebootPollTimer) clearInterval(rebootPollTimer)
  if (rebootElapsedTimer) clearInterval(rebootElapsedTimer)
})

async function performReboot() {
  if (!rebootPassword.value) return
  rebooting.value = true
  rebootError.value = ''
  try {
    await rpcClient.call({ method: 'system.reboot', params: { password: rebootPassword.value } })
    showRebootConfirm.value = false
    rebootPassword.value = ''
    startRebootOverlay()
  } catch (e) {
    rebootError.value = e instanceof Error ? e.message : 'Reboot failed'
    rebooting.value = false
  }
}

// Factory Reset
const showFactoryResetConfirm = ref(false)
const factoryResetLoading = ref(false)

async function performFactoryReset() {
  factoryResetLoading.value = true
  try {
    await rpcClient.call({ method: 'system.factory-reset', params: { confirm: true } })
    localStorage.clear()
    showFactoryResetConfirm.value = false
    router.push('/onboarding/intro')
  } catch {
    localStorage.clear()
    showFactoryResetConfirm.value = false
    router.push('/onboarding/intro')
  }
}
</script>

<template>
  <!-- Network Diagnostics Link -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-3">
      <div>
        <h2 class="text-xl font-semibold text-white/96 mb-1">{{ t('common.network') }}</h2>
        <p class="text-sm text-white/60">{{ t('settings.networkDesc') }}</p>
      </div>
      <button @click="router.push('/dashboard/server')" class="glass-button px-4 py-2 rounded-lg text-sm flex items-center justify-center gap-2 w-full md:w-auto shrink-0">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 7l5 5m0 0l-5 5m5-5H6" />
        </svg>
        {{ t('common.networkDiagnostics') }}
      </button>
    </div>
  </div>

  <!-- Reboot Section -->
  <div class="glass-card px-6 py-6 mb-6">
    <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-3">
      <div>
        <h2 class="text-xl font-semibold text-white/96 mb-1">Reboot</h2>
        <p class="text-sm text-white/60">Restart the machine. All containers will restart automatically.</p>
      </div>
      <button
        class="glass-button px-6 py-2 rounded-lg text-sm w-full md:w-auto shrink-0"
        :disabled="rebooting"
        @click="showRebootConfirm = true"
      >
        {{ rebooting ? 'Rebooting...' : 'Reboot' }}
      </button>
    </div>
  </div>

  <!-- Reboot Confirmation Modal -->
  <Teleport to="body">
    <div v-if="showRebootConfirm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm" @click.self="showRebootConfirm = false">
      <div class="glass-card px-8 py-8 max-w-md mx-4">
        <h3 class="text-lg font-semibold text-white/90 mb-3">Reboot Node</h3>
        <p class="text-sm text-white/60 mb-4">Enter your password to confirm reboot. The node will be temporarily unavailable.</p>
        <input
          v-model="rebootPassword"
          type="password"
          class="w-full px-3 py-2 rounded-lg bg-white/10 text-white border border-white/20 focus:border-orange-500 focus:ring-1 focus:ring-orange-500 mb-4"
          placeholder="Password"
          @keydown.enter="performReboot"
        />
        <p v-if="rebootError" class="text-sm text-red-400 mb-3">{{ rebootError }}</p>
        <div class="flex gap-3 justify-end">
          <button class="glass-button" @click="showRebootConfirm = false">Cancel</button>
          <button
            class="glass-button px-6"
            :disabled="rebooting || !rebootPassword"
            @click="performReboot"
          >
            {{ rebooting ? 'Rebooting...' : 'Confirm Reboot' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- Reboot Progress Overlay -->
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="rebootOverlay"
        class="fixed inset-0 z-[3000] bg-black flex flex-col items-center justify-center overflow-hidden"
      >
        <!-- Centered animated ring + logo — same composition as the screensaver -->
        <div class="reboot-ring-content">
          <ScreensaverRing />
          <div class="reboot-logo-wrapper">
            <ScreensaverLogo />
          </div>
        </div>

        <!-- Stage text + progress bar underneath -->
        <div class="mt-8 w-[min(520px,80vw)] text-center">
          <h2 class="text-xl font-semibold text-white mb-1">
            {{ rebootStage === 'rebooting' ? 'Rebooting…'
             : rebootStage === 'reconnecting' ? 'Reconnecting to your node…'
             : rebootStage === 'ready' ? 'Back online'
             : 'Reboot is taking longer than expected' }}
          </h2>
          <p class="text-sm text-white/60 mb-4">
            Your node is restarting. This page will refresh automatically once it's back.
          </p>

          <!-- Animated progress bar: indeterminate stripe while working,
               solid green when ready, paused at half while stalled. -->
          <div class="w-full h-2 bg-white/10 rounded-full overflow-hidden mb-3 relative">
            <div v-if="rebootStage === 'ready'" class="absolute inset-0 bg-green-400"></div>
            <div v-else-if="rebootStage === 'stalled'" class="absolute inset-y-0 left-0 w-1/2 bg-orange-400/60"></div>
            <div v-else class="absolute inset-y-0 w-1/3 bg-orange-400 rounded-full reboot-overlay-bar-anim"></div>
          </div>

          <p class="text-xs text-white/40">{{ rebootElapsedLabel }}</p>

          <button
            v-if="rebootStage === 'stalled'"
            @click="rebootReloadNow"
            class="mt-5 glass-button rounded-lg px-5 py-2 text-sm font-medium bg-orange-500/20 border-orange-400/30"
          >
            Reload now
          </button>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- Factory Reset Section -->
  <div class="glass-card px-6 py-6 mb-6 border border-red-500/30">
    <div class="flex flex-col md:flex-row md:items-center md:justify-between gap-3">
      <div>
        <h2 class="text-xl font-semibold text-red-400/90 mb-1">Factory Reset</h2>
        <p class="text-sm text-white/60">
          Wipe all user data, identities, and credentials. Container images are preserved. The node will restart and show the onboarding screen.
        </p>
      </div>
      <button
        class="glass-button glass-button-danger w-full md:w-auto shrink-0"
        @click="showFactoryResetConfirm = true"
      >
        Factory Reset
      </button>
    </div>
  </div>

  <!-- Factory Reset Confirmation Modal -->
  <Teleport to="body">
    <div v-if="showFactoryResetConfirm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="glass-card px-8 py-8 max-w-md mx-4">
        <h3 class="text-lg font-semibold text-white/90 mb-3">Are you sure?</h3>
        <p class="text-sm text-white/60 mb-6">
          This will delete all identities, credentials, and settings. This cannot be undone.
        </p>
        <div class="flex gap-3 justify-end">
          <button class="glass-button" @click="showFactoryResetConfirm = false">Cancel</button>
          <button
            class="glass-button glass-button-danger"
            :disabled="factoryResetLoading"
            @click="performFactoryReset"
          >
            {{ factoryResetLoading ? 'Resetting...' : 'Yes, Reset' }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.reboot-ring-content {
  position: relative;
  display: grid;
  place-items: center;
}
.reboot-logo-wrapper {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  z-index: 10;
  filter: drop-shadow(0 0 40px rgba(255, 255, 255, 0.15));
}

.reboot-overlay-bar-anim {
  animation: rebootBarSlide 1.8s ease-in-out infinite;
}
@keyframes rebootBarSlide {
  0%   { transform: translateX(-100%); }
  50%  { transform: translateX(120%); }
  100% { transform: translateX(300%); }
}
</style>
