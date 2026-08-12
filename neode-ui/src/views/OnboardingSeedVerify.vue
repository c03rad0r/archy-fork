<template>
  <div class="h-[100dvh] flex items-center justify-center p-3 sm:p-4 md:p-6">
    <div class="max-w-[800px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Header (hidden after verification) -->
      <div v-if="!verified" class="text-center flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6 pb-2 sm:pb-3">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-1.5 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Verify Your Seed
        </h1>
        <p class="text-xs sm:text-sm md:text-base text-white/75 leading-relaxed max-w-[600px] mx-auto">
          Confirm you wrote down your seed correctly by entering the requested words.
        </p>
      </div>

      <!-- Scrollable Content -->
      <div class="flex-1 overflow-y-auto overflow-x-hidden min-h-0 px-6 sm:px-8 py-4">
        <div class="flex flex-col items-center gap-3 sm:gap-4">
          <p v-if="errorMessage" class="text-red-400 text-sm">{{ errorMessage }}</p>

          <!-- Verification Success -->
          <div v-if="verified" class="w-full max-w-[600px] pt-2">
            <div class="text-center mb-4">
              <div class="flex justify-center mb-4">
                <div class="path-option-card cursor-default w-16 h-16 sm:w-20 sm:h-20 rounded-full flex items-center justify-center">
                  <svg class="w-8 h-8 sm:w-10 sm:h-10 text-black" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
              </div>
              <p class="text-base sm:text-[20px] text-white/80 leading-relaxed max-w-[600px] mx-auto mb-2">
                Seed verified successfully
              </p>
            </div>

            <!-- DID -->
            <div class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-5 mb-3">
              <div class="text-left w-full">
                <div class="flex items-center justify-between mb-2">
                  <h3 class="text-xs sm:text-sm font-semibold text-white/80 uppercase tracking-wide">Your DID</h3>
                  <button @click="copyText(did)" class="text-xs text-white/40 hover:text-white/70 transition-colors">
                    {{ copiedField === 'did' ? 'Copied!' : 'Copy' }}
                  </button>
                </div>
                <div class="bg-black/40 rounded-lg p-3 sm:p-4 backdrop-blur-sm border border-white/10">
                  <p class="text-white/95 font-mono text-xs sm:text-sm break-all leading-relaxed">{{ did }}</p>
                </div>
                <p class="text-xs text-white/50 mt-2">For Web5, federation, and verifiable credentials</p>
              </div>
            </div>

            <!-- Nostr npub -->
            <div v-if="nostrNpub" class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-5">
              <div class="text-left w-full">
                <div class="flex items-center justify-between mb-2">
                  <h3 class="text-xs sm:text-sm font-semibold text-white/80 uppercase tracking-wide">Your Nostr ID</h3>
                  <button @click="copyText(nostrNpub)" class="text-xs text-white/40 hover:text-white/70 transition-colors">
                    {{ copiedField === 'npub' ? 'Copied!' : 'Copy' }}
                  </button>
                </div>
                <div class="bg-black/40 rounded-lg p-3 sm:p-4 backdrop-blur-sm border border-white/10">
                  <p class="text-white/95 font-mono text-xs sm:text-sm break-all leading-relaxed">{{ nostrNpub }}</p>
                </div>
                <p class="text-xs text-white/50 mt-2">For Nostr social apps and NIP-07 signing</p>
              </div>
            </div>

          </div>

          <!-- Word Input Fields -->
          <div v-if="!verified" class="w-full max-w-[600px] space-y-3 sm:space-y-4">
            <div
              v-for="(idx, i) in challengeIndices"
              :key="idx"
            >
              <label class="block text-xs font-semibold text-white/80 mb-1.5 uppercase tracking-wide text-left">
                Word #{{ idx + 1 }}
              </label>
              <input
                :ref="el => { if (el) inputRefs[i] = el as HTMLInputElement }"
                v-model="answers[i]"
                type="text"
                autocomplete="off"
                autocapitalize="none"
                spellcheck="false"
                :placeholder="`Enter word #${idx + 1}`"
                class="w-full bg-black/40 border border-white/10 rounded-lg px-3 py-2.5 text-white/95 placeholder-white/40 focus:outline-none focus:ring-1 focus:ring-inset focus:ring-white/30 focus:bg-black/50 transition-all font-mono text-[1.2rem]"
                @keydown.enter.prevent="i < challengeIndices.length - 1 ? inputRefs[i + 1]?.focus() : verify()"
              />
            </div>
          </div>
        </div>
      </div>

      <!-- Fixed Footer -->
      <div class="flex-shrink-0 flex items-center justify-center gap-4 max-w-[600px] mx-auto w-full px-6 sm:px-8 pt-3 pb-4 sm:pb-6">
        <span
          v-if="!verified"
          @click="goBack"
          class="path-action-button path-action-button--continue cursor-pointer select-none inline-flex items-center justify-center"
        >
          Back
        </span>
        <button
          v-if="!verified"
          @click="verify"
          type="button"
          :disabled="isVerifying || !allFilled"
          class="path-action-button path-action-button--continue disabled:opacity-50"
        >
          <span v-if="isVerifying">Verifying...</span>
          <span v-else>Verify</span>
        </button>
        <span
          v-if="verified"
          @click="downloadIdentity"
          class="path-action-button path-action-button--continue cursor-pointer select-none inline-flex items-center justify-center gap-2"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
          </svg>
          Download
        </span>
        <button
          v-if="verified"
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
import { ref, computed, onMounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const continueButton = ref<HTMLButtonElement | null>(null)
const inputRefs = ref<HTMLInputElement[]>([])
const words = ref<string[]>([])
const challengeIndices = ref<number[]>([])
const answers = ref<string[]>(['', '', '', ''])
const verified = ref(false)
const isVerifying = ref(false)
const errorMessage = ref('')
const did = ref('')
const nostrNpub = ref('')
const copiedField = ref('')

const allFilled = computed(() => answers.value.every(a => a.trim().length > 0))

function pickRandomIndices(count: number, max: number): number[] {
  const indices = new Set<number>()
  while (indices.size < count) {
    indices.add(Math.floor(Math.random() * max))
  }
  return Array.from(indices).sort((a, b) => a - b)
}

onMounted(() => {
  const stored = sessionStorage.getItem('_seed_words')
  if (!stored) {
    router.replace('/onboarding/seed').catch(() => {})
    return
  }
  words.value = JSON.parse(stored)

  // Restore challenge indices so going back and returning asks the same words
  const savedIndices = sessionStorage.getItem('_seed_challenge_indices')
  if (savedIndices) {
    try {
      const parsed = JSON.parse(savedIndices)
      if (Array.isArray(parsed) && parsed.length === 4) {
        challengeIndices.value = parsed
      } else {
        challengeIndices.value = pickRandomIndices(4, 24)
      }
    } catch {
      challengeIndices.value = pickRandomIndices(4, 24)
    }
  } else {
    challengeIndices.value = pickRandomIndices(4, 24)
  }
  sessionStorage.setItem('_seed_challenge_indices', JSON.stringify(challengeIndices.value))

  nextTick(() => {
    setTimeout(() => inputRefs.value[0]?.focus({ preventScroll: true }), 300)
  })
})

function goBack() {
  playNavSound('action')
  router.push('/onboarding/seed').catch(() => {})
}

function copyText(text: string) {
  navigator.clipboard.writeText(text).catch(() => {})
  copiedField.value = text === did.value ? 'did' : 'npub'
  setTimeout(() => { copiedField.value = '' }, 2000)
}

function downloadIdentity() {
  const data = {
    did: did.value,
    nostr_npub: nostrNpub.value || undefined,
    created: new Date().toISOString(),
  }
  const json = JSON.stringify(data, null, 2)
  const blob = new Blob([json], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'archipelago-identity.json'
  a.style.display = 'none'
  document.body.appendChild(a)
  a.click()
  setTimeout(() => { document.body.removeChild(a); URL.revokeObjectURL(url) }, 1000)
}

async function verify() {
  if (!allFilled.value) return
  isVerifying.value = true
  errorMessage.value = ''

  const correct = challengeIndices.value.every(
    (wordIdx, i) => (answers.value[i] ?? '').trim().toLowerCase() === (words.value[wordIdx] ?? '')
  )

  if (!correct) {
    isVerifying.value = false
    errorMessage.value = 'One or more words are incorrect. Please check your written seed and try again.'
    return
  }

  try {
    const res = await rpcClient.call<{ verified: boolean; did: string; nostr_npub: string }>({
      method: 'seed.verify',
      params: { words: words.value, indices: challengeIndices.value },
    })

    if (res.verified) {
      verified.value = true
      did.value = res.did
      nostrNpub.value = res.nostr_npub || ''
      localStorage.setItem('neode_did', res.did)
      if (res.nostr_npub) localStorage.setItem('neode_nostr_npub', res.nostr_npub)
      sessionStorage.removeItem('_seed_words')
      sessionStorage.removeItem('_seed_challenge_indices')

      nextTick(() => {
        setTimeout(() => continueButton.value?.focus({ preventScroll: true }), 100)
      })
    } else {
      errorMessage.value = 'Verification failed. Please try again.'
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : ''
    // Words already matched the local copy, so a network/server hiccup here is
    // transient — the backend just needs a moment. Don't alarm the user; let
    // them tap Verify again.
    if (/502|503|504|timeout|fetch|network|Failed to fetch|Request failed/i.test(msg)) {
      errorMessage.value = 'Server is still starting. Please tap Verify again in a moment.'
    } else {
      errorMessage.value = msg || 'Verification failed'
    }
  } finally {
    isVerifying.value = false
  }
}

function proceed() {
  playNavSound('action')
  router.push('/onboarding/identity').catch(() => {})
}
</script>
