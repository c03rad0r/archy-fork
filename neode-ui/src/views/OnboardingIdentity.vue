<template>
  <div class="h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <div class="max-w-[800px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Header -->
      <div class="text-center flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-2 sm:mb-4 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Name your identity
        </h1>
        <p class="text-sm sm:text-base md:text-[20px] text-white/75 leading-relaxed max-w-[600px] mx-auto mb-4 sm:mb-6">
          Give your first identity a name and choose how you'll use it. You can create more identities later.
        </p>
      </div>

      <!-- Scrollable Content -->
      <div class="flex-1 overflow-y-auto overflow-x-hidden min-h-0">
      <div class="flex flex-col items-center gap-4 sm:gap-6 mb-4 sm:mb-6 px-3 sm:px-4">
        <div class="w-full max-w-[600px] space-y-4 sm:space-y-6">
          <!-- Name Input -->
          <div class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-6">
            <label class="block text-sm font-semibold text-white/80 mb-3 uppercase tracking-wide">Identity Name</label>
            <input
              ref="nameInput"
              v-model="identityName"
              type="text"
              placeholder="Personal"
              class="w-full bg-black/40 border border-white/10 rounded-lg px-4 py-3 text-white/95 placeholder-white/40 focus:outline-none focus:border-white/30 focus:bg-black/50 transition-all"
            />
          </div>

          <!-- Purpose Selection -->
          <div class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-6">
            <label class="block text-sm font-semibold text-white/80 mb-3 uppercase tracking-wide">Purpose</label>
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
              <button
                v-for="p in purposes"
                :key="p.value"
                @click="playNavSound('action'); selectedPurpose = p.value"
                class="px-4 py-3 rounded-lg border text-left transition-all"
                :class="selectedPurpose === p.value
                  ? 'bg-white/15 border-white/30 text-white'
                  : 'bg-black/20 border-white/10 text-white/60 hover:bg-white/10 hover:text-white/80'"
              >
                <div class="flex items-center gap-2 mb-1">
                  <div class="w-5 h-5 rounded-full flex items-center justify-center shrink-0" :class="p.color">
                    <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
                    </svg>
                  </div>
                  <span class="font-medium text-sm">{{ p.label }}</span>
                </div>
                <p class="text-xs text-white/50 ml-7">{{ p.desc }}</p>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Error / Server starting -->
      <div v-if="serverStarting" class="text-center mb-4 px-3">
        <p class="text-orange-400/80 text-sm">Server is still starting up. Your identity will be saved once it's ready.</p>
      </div>
      <p v-else-if="errorMessage" class="text-red-400 text-sm text-center mb-4">{{ errorMessage }}</p>
      </div>

      <!-- Action Buttons -->
      <div class="flex justify-center max-w-[600px] mx-auto flex-shrink-0 px-3 sm:px-4 pb-4 sm:pb-6 pt-3">
        <button
          @click="createIdentity"
          :disabled="isCreating"
          class="path-action-button path-action-button--continue"
        >
          <span v-if="isCreating">Creating...</span>
          <span v-else>Continue</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const nameInput = ref<HTMLInputElement | null>(null)
const identityName = ref('Personal')

onMounted(() => {
  setTimeout(() => {
    nameInput.value?.focus({ preventScroll: true })
  }, 500)
})
const selectedPurpose = ref('personal')
const isCreating = ref(false)
const errorMessage = ref('')
const serverStarting = ref(false)

const purposes = [
  { value: 'personal', label: 'Personal', desc: 'Everyday use', color: 'bg-blue-500/30 text-blue-400' },
  { value: 'business', label: 'Business', desc: 'Professional', color: 'bg-orange-500/30 text-orange-400' },
  { value: 'anonymous', label: 'Anonymous', desc: 'Private', color: 'bg-purple-500/30 text-purple-400' },
]

function isServerStartingError(err: unknown): boolean {
  const msg = err instanceof Error ? err.message : String(err)
  return /502|503|504|timeout|fetch|network|Failed to fetch/i.test(msg)
}

async function createIdentity() {
  isCreating.value = true
  errorMessage.value = ''
  serverStarting.value = false
  try {
    await rpcClient.call({
      method: 'identity.create',
      params: {
        name: identityName.value || 'Personal',
        purpose: selectedPurpose.value
      }
    })
    playNavSound('action')
    router.push('/onboarding/done').catch(() => {})
  } catch (err) {
    if (isServerStartingError(err)) {
      serverStarting.value = true
    } else {
      errorMessage.value = err instanceof Error ? err.message : 'Failed to create identity'
    }
  } finally {
    isCreating.value = false
  }
}

</script>
