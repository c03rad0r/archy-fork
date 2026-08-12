<template>
  <div class="min-h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <!-- Main Glass Container -->
    <div class="max-w-[800px] w-full relative z-10 path-glass-container onb-scroll-container">
      <!-- Header -->
      <div v-if="!verified" class="text-center mb-4 sm:mb-6 flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-2 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Verify Your Identity
        </h1>
        <p class="text-sm sm:text-base md:text-[20px] text-white/75 leading-relaxed max-w-[600px] mx-auto">
          Sign a challenge to verify your decentralized identity is working correctly.
        </p>
      </div>

      <!-- Content Area -->
      <div class="flex flex-col items-center gap-6 mb-6">
        <p v-if="serverStarting" class="text-orange-400/80 text-sm">Server is still starting up. Please try again shortly.</p>
        <p v-else-if="errorMessage" class="text-red-400 text-sm">{{ errorMessage }}</p>
        <!-- Sign Button (if not verified yet) -->
        <button
          ref="signButton"
          v-if="!verified"
          @click="signChallenge"
          :disabled="isSigning"
          class="path-action-button path-action-button--continue"
        >
          <span v-if="!isSigning">Sign Challenge</span>
          <span v-else class="flex items-center gap-2">
            <svg class="animate-spin h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Signing...
          </span>
        </button>

        <!-- Verification Success -->
        <div v-if="verified" class="w-full max-w-[600px]">
          <div class="text-center mb-6">
            <div class="flex justify-center mb-6">
              <div class="path-option-card cursor-default w-20 h-20 rounded-full flex items-center justify-center">
                <svg class="w-10 h-10 text-black" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
              </div>
            </div>
            <p class="text-[20px] text-white/80 leading-relaxed max-w-[600px] mx-auto mb-6">
              Your identity has been successfully verified and is ready to use.
            </p>
          </div>

          <!-- Signature Display -->
          <div class="path-option-card cursor-default px-6 py-6">
            <div class="text-left">
              <h3 class="text-sm font-semibold text-white/80 mb-2 uppercase tracking-wide">Signature</h3>
              <div class="bg-black/40 rounded-lg p-4 backdrop-blur-sm border border-white/10">
                <p class="text-white/95 font-mono text-xs break-all leading-relaxed">
                  {{ signature }}
                </p>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Action Buttons -->
      <div class="flex justify-center max-w-[600px] mx-auto flex-shrink-0 px-3 sm:px-4 pb-4 sm:pb-6">
        <button
          ref="finishButton"
          v-if="verified"
          @click="proceed"
          class="path-action-button path-action-button--continue"
        >
          Finish
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { completeOnboarding } from '@/composables/useOnboarding'
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const signButton = ref<HTMLButtonElement | null>(null)
const finishButton = ref<HTMLButtonElement | null>(null)
const verified = ref(false)

onMounted(() => {
  setTimeout(() => {
    signButton.value?.focus({ preventScroll: true })
  }, 500)
})
const isSigning = ref(false)
const signature = ref('')
const currentChallenge = ref('')
const errorMessage = ref('')
const serverStarting = ref(false)

/** Generate a cryptographically random challenge (32 bytes, base64) */
function generateChallenge(): string {
  const bytes = new Uint8Array(32)
  crypto.getRandomValues(bytes)
  return btoa(String.fromCharCode(...bytes))
}

async function signChallenge() {
  isSigning.value = true
  errorMessage.value = ''

  for (let attempt = 0; attempt < 3; attempt++) {
    try {
      currentChallenge.value = generateChallenge()
      const { signature: sig } = await rpcClient.signChallenge(currentChallenge.value)
      signature.value = sig
      isSigning.value = false

      // Auto-verify the signature using identity.verify
      const did = localStorage.getItem('neode_did')
      if (did) {
        const result = await rpcClient.call({
          method: 'identity.verify',
          params: { did, message: currentChallenge.value, signature: sig },
        }) as { valid: boolean }
        verified.value = result.valid !== false
      } else {
        verified.value = true
      }
      nextTick(() => {
        setTimeout(() => finishButton.value?.focus({ preventScroll: true }), 100)
      })
      return
    } catch (err) {
      const msg = err instanceof Error ? err.message : ''
      const isRetryable = /502|503|504|timeout|fetch|network|Failed to fetch/i.test(msg)
      if (!isRetryable || attempt === 2) {
        if (isRetryable) {
          serverStarting.value = true
        } else {
          errorMessage.value = msg || 'Failed to sign challenge. Please try again.'
        }
      } else {
        await new Promise((r) => setTimeout(r, 1000 * (attempt + 1)))
      }
    }
  }
  isSigning.value = false
}

async function proceed() {
  playNavSound('action')
  try {
    await completeOnboarding()
  } catch {
    /* localStorage fallback ensures we can proceed */
  }
  router.push('/onboarding/done').catch(() => {})
}

</script>

