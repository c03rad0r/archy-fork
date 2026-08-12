<template>
  <div class="h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <!-- Main Glass Container - Scrollable -->
    <div class="max-w-[800px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Success Content -->
      <div class="flex-1 overflow-y-auto overflow-x-hidden min-h-0 text-center space-y-4 sm:space-y-6 px-3 sm:px-4 py-4 sm:py-6">
        <!-- Success Icon -->
        <div class="flex justify-center mb-4 sm:mb-6">
          <div class="path-option-card cursor-default w-16 h-16 sm:w-20 sm:h-20 rounded-full flex items-center justify-center">
            <svg class="w-10 h-10 text-black" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
              <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
            </svg>
          </div>
        </div>

        <!-- Description -->
        <p class="text-sm sm:text-base md:text-[20px] text-white/80 leading-relaxed max-w-[600px] mx-auto mb-6 sm:mb-8">
          Your sovereign identity is ready. You can now log in and start your journey as a noderunner.
        </p>

        <!-- Features Grid -->
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3 sm:gap-4 mb-6 sm:mb-8 max-w-[700px] mx-auto">
          <div class="path-option-card cursor-default py-6">
            <svg class="w-10 h-10 mx-auto mb-3 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
            </svg>
            <h3 class="text-sm font-semibold text-white/90">Sovereign Identity</h3>
          </div>

          <div class="path-option-card cursor-default py-6">
            <svg class="w-10 h-10 mx-auto mb-3 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
            <h3 class="text-sm font-semibold text-white/90">Encrypted Backup</h3>
          </div>

          <div class="path-option-card cursor-default py-6">
            <svg class="w-10 h-10 mx-auto mb-3 text-white/80" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
            </svg>
            <h3 class="text-sm font-semibold text-white/90">Ready to Use</h3>
          </div>
        </div>

        <!-- FIPS activation status (non-blocking — onboarding never waits on it) -->
        <div v-if="fipsLabel" class="flex items-center justify-center gap-2 mb-4 text-xs sm:text-sm">
          <span
            class="w-2 h-2 rounded-full"
            :class="fipsReady ? 'bg-green-400' : 'bg-orange-400 animate-pulse'"
          ></span>
          <span class="text-white/60">{{ fipsLabel }}</span>
        </div>

        <!-- Set Password Button -->
        <p class="text-xs text-white/50 mb-3">You'll create your node password next</p>
        <button
          ref="setPasswordButton"
          @click="goToLogin"
          class="path-action-button path-action-button--continue mx-auto"
        >
          Set Password
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const setPasswordButton = ref<HTMLButtonElement | null>(null)

// FIPS auto-activates in a detached task after the seed is written, so it can
// lag a few seconds behind onboarding on slow hardware. Surface a gentle status
// so the encrypted-transport bring-up reads as "in progress", never "stuck".
// This is purely informational — it never blocks moving on to set a password.
const fipsLabel = ref('')
const fipsReady = ref(false)
let fipsTimer: ReturnType<typeof setTimeout> | null = null
let fipsTries = 0

async function pollFips() {
  try {
    const s = await rpcClient.call<{ key_present?: boolean; service_active?: boolean }>({
      method: 'fips.status',
      timeout: 8000,
    })
    // Only relevant once a seed-derived FIPS key exists; otherwise stay silent.
    if (s.key_present) {
      if (s.service_active) {
        fipsReady.value = true
        fipsLabel.value = 'Private connection ready'
        return // settled — stop polling
      }
      fipsLabel.value = 'Securing your private connection…'
    }
  } catch {
    // Backend still booting — ignore and keep polling.
  }
  fipsTries += 1
  if (fipsTries < 20) {
    fipsTimer = setTimeout(pollFips, 3000)
  }
}

onMounted(() => {
  setTimeout(() => {
    setPasswordButton.value?.focus({ preventScroll: true })
  }, 500)
  pollFips()
})

onUnmounted(() => {
  if (fipsTimer) clearTimeout(fipsTimer)
})

function goToLogin() {
  playNavSound('action')
  // The login that follows the wizard gets the full dashboard entrance
  // (zoom + oomph) even when it's a regular password login (e.g. the demo) —
  // Login.vue consumes this flag on success.
  try { sessionStorage.setItem('archy_onboarding_finale', '1') } catch { /* ignore */ }
  router.push('/login').catch(() => {})
}
</script>

