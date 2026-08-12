<template>
  <div class="h-full flex items-center justify-center p-3 sm:p-4 md:p-6 relative">
    <!-- Main Glass Container - Scrollable -->
    <div class="max-w-[1200px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Scrollable Content -->
      <div class="flex-1 overflow-y-auto overflow-x-hidden min-h-0">
      <!-- Header -->
      <div class="text-center mb-4 md:mb-6 flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6">
        <h1 class="text-xl md:text-[26px] font-semibold text-white/96 mb-2 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">Your Node, Your Possibilities</h1>
        <p class="text-xs md:text-sm text-white/75 leading-relaxed">Archipelago gives you the tools to build your sovereign digital life. All of these capabilities are available from your dashboard.</p>
      </div>

      <!-- Options Grid -->
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3 md:gap-4 flex-shrink-0 mb-4 md:mb-6 px-3 sm:px-4">
        <!-- Self Sovereignty -->
        <div class="path-option-card">
          <div class="icon-wrapper transition-all duration-300">
            <svg class="w-10 h-10 text-white/90" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
            </svg>
          </div>
          <h3 class="text-lg font-semibold text-white/96 mb-1.5">Self Sovereignty</h3>
          <p class="text-sm text-white/75 leading-snug">
            Data, files, ownership, property of my data estate. Own, manage, edit, and even sell your personal data.
          </p>
        </div>

        <!-- Community Commerce -->
        <div class="path-option-card">
          <svg class="w-12 h-12 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
          </svg>
          <h3 class="text-xl font-semibold text-white/96 mb-2">Community Commerce</h3>
          <p class="text-[15px] text-white/75 leading-snug">
            Self contained and owned ecommerce system built on bitcoin and mesh networks. Trade freely without intermediaries.
          </p>
        </div>

        <!-- Sovereign Projects -->
        <div class="path-option-card">
          <svg class="w-12 h-12 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 13.255A23.931 23.931 0 0112 15c-3.183 0-6.22-.62-9-1.745M16 6V4a2 2 0 00-2-2h-4a2 2 0 00-2 2v2m4 6h.01M5 20h14a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
          </svg>
          <h3 class="text-xl font-semibold text-white/96 mb-2">Sovereign Projects</h3>
          <p class="text-[15px] text-white/75 leading-snug">
            Logistics and project management self owned with privacy control. Collaborate without surveillance.
          </p>
        </div>

        <!-- Data Transmitter -->
        <div class="path-option-card">
          <svg class="w-12 h-12 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0" />
          </svg>
          <h3 class="text-xl font-semibold text-white/96 mb-2">Data Transmitter</h3>
          <p class="text-[15px] text-white/75 leading-snug">
            Assist the new sovereign net with relay points and networking where you get paid for your value.
          </p>
        </div>

        <!-- Hoster -->
        <div class="path-option-card">
          <svg class="w-12 h-12 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />
          </svg>
          <h3 class="text-xl font-semibold text-white/96 mb-2">Hoster</h3>
          <p class="text-[15px] text-white/75 leading-snug">
            Host services and content, archives, and more to others for micro bitcoin payments. Earn while you serve.
          </p>
        </div>

        <!-- Sovereign AI -->
        <div class="path-option-card">
          <svg class="w-12 h-12 mb-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z" />
          </svg>
          <h3 class="text-xl font-semibold text-white/96 mb-2">Sovereign AI</h3>
          <p class="text-[15px] text-white/75 leading-snug">
            Run AI models locally on your hardware. No cloud surveillance, complete privacy, full control over your AI assistant.
          </p>
        </div>
      </div>
      </div>

      <!-- Action Buttons -->
      <div class="flex justify-center max-w-[600px] mx-auto flex-shrink-0 px-3 sm:px-4 pb-4 sm:pb-6 pt-4 sm:pt-6">
        <button
          ref="continueButton"
          @click="proceed"
          class="path-action-button path-action-button--continue"
        >
          Continue
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const continueButton = ref<HTMLButtonElement | null>(null)

onMounted(() => {
  setTimeout(() => {
    continueButton.value?.focus({ preventScroll: true })
  }, 500)
})

function proceed() {
  playNavSound('action')
  router.push('/onboarding/seed').catch(() => {})
}
</script>
