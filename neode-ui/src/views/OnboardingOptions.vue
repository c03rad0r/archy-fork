<template>
  <div class="h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <div class="max-w-[1200px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <div class="flex-1 overflow-y-auto overflow-x-hidden min-h-0">
      <div class="text-center mb-4 sm:mb-6 flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6">
        <div class="logo-gradient-border inline-block mb-4 sm:mb-6">
          <img
            src="/assets/icon/favico-black-v2.svg"
            alt="Archipelago"
            class="w-16 h-16 sm:w-20 sm:h-20"
          />
        </div>
        <h1 class="text-2xl sm:text-4xl font-bold text-white mb-2 sm:mb-4">Choose Your Setup</h1>
        <p class="text-base sm:text-xl text-white/80">How would you like to get started?</p>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 gap-3 sm:gap-6 px-3 sm:px-4 max-w-[760px] mx-auto w-full">
        <!-- Fresh Start -->
        <button
          @click="selectOption('fresh')"
          class="path-option-card text-center"
          :class="{ 'path-option-card--selected': selected === 'fresh' }"
        >
          <div class="mb-3 sm:mb-4">
            <div class="w-12 h-12 sm:w-16 sm:h-16 mx-auto bg-white/10 rounded-full flex items-center justify-center">
              <svg class="w-6 h-6 sm:w-8 sm:h-8 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
            </div>
          </div>
          <h3 class="text-lg sm:text-xl font-semibold text-white mb-1 sm:mb-2">Fresh Start</h3>
          <p class="text-white/70 text-xs sm:text-sm">
            Set up a new server from scratch
          </p>
        </button>

        <!-- Restore from Seed -->
        <button
          @click="selectOption('restore')"
          class="path-option-card text-center"
          :class="{ 'path-option-card--selected': selected === 'restore' }"
        >
          <div class="mb-3 sm:mb-4">
            <div class="w-12 h-12 sm:w-16 sm:h-16 mx-auto bg-white/10 rounded-full flex items-center justify-center">
              <svg class="w-6 h-6 sm:w-8 sm:h-8 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
              </svg>
            </div>
          </div>
          <h3 class="text-lg sm:text-xl font-semibold text-white mb-1 sm:mb-2">Restore from Seed</h3>
          <p class="text-white/70 text-xs sm:text-sm">
            Enter your 24-word recovery phrase
          </p>
        </button>
      </div>
      </div>

      <div class="flex justify-center flex-shrink-0 px-3 sm:px-4 pb-4 sm:pb-6 pt-4 sm:pt-6">
        <button
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
const selected = ref<string | null>(null)

onMounted(() => {
  selected.value = 'fresh'
})

function selectOption(option: string) {
  selected.value = option
}

async function proceed() {
  playNavSound('action')
  if (selected.value === 'restore') {
    router.push('/onboarding/seed-restore').catch(() => {})
  } else {
    router.push('/onboarding/seed').catch(() => {})
  }
}
</script>
