<template>
  <div class="min-h-full flex items-center justify-center p-4 sm:p-6">
    <div class="max-w-2xl w-full">
      <div class="glass-card p-8 pt-16 sm:p-12 sm:pt-20 text-center relative overflow-visible onb-card">
        <!-- Logo - half in, half out of container -->
        <div class="absolute -top-8 sm:-top-10 left-0 right-0 flex justify-center z-10 onb-logo">
          <div class="logo-gradient-border logo-gloss w-16 h-16 sm:w-20 sm:h-20">
            <AnimatedLogo no-border fit />
          </div>
        </div>

        <h1 class="text-2xl sm:text-4xl font-bold text-white mb-3 sm:mb-4 onb-title">
          Welcome to Archipelago
        </h1>

        <p class="text-base sm:text-xl text-white/80 mb-8 sm:mb-12 max-w-2xl mx-auto onb-tagline">
          Your personal server for a sovereign digital life
        </p>

        <button
          ref="ctaButton"
          @click="goToOptions"
          class="glass-button px-6 py-3 sm:px-8 sm:py-4 rounded-lg text-base sm:text-lg font-medium transition-all hover:bg-black/70 hover:border-white/30 onb-cta"
        >
          {{ isDemo ? 'Enter the demo →' : 'Unlock your sovereignty →' }}
        </button>

        <!-- Onboarding wizard entry points are hidden in the demo (no seed/identity setup) -->
        <template v-if="!isDemo">
          <a
            tabindex="0"
            role="button"
            class="text-white/50 hover:text-white/80 underline text-sm cursor-pointer mt-4 block text-center onb-cta"
            @click="goToRestore"
            @keydown.enter="goToRestore"
          >
            Restore from seed phrase
          </a>
          <a
            tabindex="0"
            role="button"
            class="text-white/50 hover:text-white/80 underline text-sm cursor-pointer mt-2 block text-center onb-cta"
            @click="goToLogin"
            @keydown.enter="goToLogin"
          >
            Already set up? Log in
          </a>
        </template>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import AnimatedLogo from '@/components/AnimatedLogo.vue'
import { playNavSound } from '@/composables/useNavSounds'
import { IS_DEMO } from '@/composables/useDemoIntro'

const router = useRouter()
const ctaButton = ref<HTMLButtonElement | null>(null)
const isDemo = IS_DEMO

onMounted(() => {
  // Auto-focus after entry animation completes (1.4s animation delay + 0.6s duration)
  setTimeout(() => {
    ctaButton.value?.focus({ preventScroll: true })
  }, 2100)
})

/** Any exit from the intro INTO login is the end of the cinematic — hand the
 *  login the one-shot finale flag so the dashboard plays its full first-entry
 *  reveal (zoom + oomph). OnboardingDone sets the same flag at the end of the
 *  full wizard; these are the shortcut exits that used to skip it (which is
 *  why the demo never got the big entrance). */
function armOnboardingFinale() {
  try { sessionStorage.setItem('archy_onboarding_finale', '1') } catch { /* ignore */ }
}

function goToOptions() {
  playNavSound('action')
  // Demo: skip the onboarding wizard (seed/identity setup) entirely — go straight
  // to login, which is prefilled with the demo password.
  if (isDemo) {
    localStorage.setItem('neode_onboarding_complete', '1')
    armOnboardingFinale()
    router.push('/login').catch(() => {})
    return
  }
  router.push('/onboarding/path').catch(() => {})
}

function goToRestore() {
  playNavSound('action')
  router.push('/onboarding/seed-restore').catch(() => {})
}

function goToLogin() {
  playNavSound('action')
  localStorage.setItem('neode_onboarding_complete', '1')
  armOnboardingFinale()
  router.push('/login').catch(() => {})
}
</script>

<style scoped>
.onb-card {
  opacity: 0;
  animation: onb-card-in 0.6s cubic-bezier(0.25, 0.46, 0.45, 0.94) 0.1s forwards;
}
.onb-logo {
  opacity: 0;
  animation: onb-scale-in 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94) 0.3s forwards;
}
.onb-title {
  opacity: 0;
  animation: onb-slide-up 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94) 0.7s forwards;
}
.onb-tagline {
  opacity: 0;
  animation: onb-slide-up 0.5s cubic-bezier(0.25, 0.46, 0.45, 0.94) 1.0s forwards;
}
.onb-cta {
  opacity: 0;
  animation: onb-fade-in 0.6s ease 1.4s forwards;
}

@keyframes onb-card-in {
  from { opacity: 0; transform: translateY(12px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes onb-scale-in {
  from { opacity: 0; transform: scale(0.92); }
  to { opacity: 1; transform: scale(1); }
}
@keyframes onb-slide-up {
  from { opacity: 0; transform: translateY(10px); }
  to { opacity: 1; transform: translateY(0); }
}
@keyframes onb-fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}
</style>

