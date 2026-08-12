<template>
  <div class="h-[100dvh] flex items-center justify-center p-3 sm:p-4 md:p-6">
    <div class="max-w-[800px] w-full max-h-full relative z-10 path-glass-container onb-scroll-container flex flex-col">
      <!-- Header -->
      <div class="text-center flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6 pb-2 sm:pb-3">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-1.5 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Restore from Seed
        </h1>
        <p class="text-xs sm:text-sm md:text-base text-white/75 leading-relaxed max-w-[600px] mx-auto">
          Enter your 24-word recovery seed to restore your node identity.
        </p>
      </div>

      <!-- Scrollable Content -->
      <div class="flex-1 overflow-y-auto overflow-x-hidden px-6 sm:px-8 min-h-0">
        <div class="flex flex-col items-center gap-3 sm:gap-4 py-3">
          <p v-if="errorMessage" class="text-red-400 text-sm">{{ errorMessage }}</p>
          <p v-if="serverStarting" class="text-orange-400/80 text-sm">Server is still starting up. Please try again shortly.</p>

          <!-- Restore Success -->
          <div v-if="restored" class="w-full max-w-[600px]">
            <div class="text-center mb-4">
              <div class="flex justify-center mb-4">
                <div class="path-option-card cursor-default w-16 h-16 sm:w-20 sm:h-20 rounded-full flex items-center justify-center">
                  <svg class="w-8 h-8 sm:w-10 sm:h-10 text-black" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </div>
              </div>
              <p class="text-base sm:text-[20px] text-white/80 leading-relaxed max-w-[600px] mx-auto mb-2">
                Identity restored successfully
              </p>
            </div>
            <div class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-5">
              <div class="text-left">
                <h3 class="text-xs sm:text-sm font-semibold text-white/80 mb-2 uppercase tracking-wide">Your DID</h3>
                <div class="bg-black/40 rounded-lg p-3 sm:p-4 backdrop-blur-sm border border-white/10">
                  <p class="text-white/95 font-mono text-xs sm:text-sm break-all leading-relaxed">{{ restoredDid }}</p>
                </div>
              </div>
            </div>
            <div v-if="restoredNpub" class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-5">
              <div class="text-left">
                <h3 class="text-xs sm:text-sm font-semibold text-white/80 mb-2 uppercase tracking-wide">Your Nostr ID</h3>
                <div class="bg-black/40 rounded-lg p-3 sm:p-4 backdrop-blur-sm border border-white/10">
                  <p class="text-white/95 font-mono text-xs sm:text-sm break-all leading-relaxed">{{ restoredNpub }}</p>
                </div>
                <p class="text-xs text-white/50 mt-2">For Nostr social apps and NIP-07 signing</p>
              </div>
            </div>
          </div>

          <!-- Word Input Grid -->
          <div v-if="!restored" class="w-full max-w-[600px]">
              <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-1.5 sm:gap-2">
                <div v-for="i in 24" :key="i" class="relative">
                  <span class="absolute left-3 top-1/2 -translate-y-1/2 text-white/30 text-[1rem] font-mono pointer-events-none">{{ i }}.</span>
                  <input
                    :ref="el => { if (el) wordInputs[i - 1] = el as HTMLInputElement }"
                    v-model="seedWords[i - 1]"
                    type="text"
                    autocomplete="off"
                    autocapitalize="none"
                    spellcheck="false"
                    class="w-full bg-black/40 border border-white/10 rounded-lg pl-9 pr-3 py-2 text-[1.2rem] text-white/95 font-mono placeholder-white/20 focus:outline-none focus:ring-1 focus:ring-inset focus:ring-white/30 focus:bg-black/50 transition-all"
                    :placeholder="`word ${i}`"
                    @keydown.enter="i < 24 ? wordInputs[i]?.focus() : restore()"
                    @input="onWordInput(i - 1)"
                    @paste="i === 1 ? onPaste($event) : undefined"
                  />
                </div>
              </div>
            <p class="text-xs text-white/40 mt-2 text-center">
              Paste all 24 words into the first field to auto-fill
            </p>
          </div>
        </div>
      </div>

      <!-- Fixed Footer -->
      <div class="flex-shrink-0 flex items-center justify-center gap-4 max-w-[600px] mx-auto w-full px-6 sm:px-8 pt-3 pb-4 sm:pb-6">
        <span
          v-if="!restored"
          @click="goBack"
          class="path-action-button path-action-button--continue cursor-pointer select-none inline-flex items-center justify-center"
        >
          Back
        </span>
        <button
          v-if="!restored"
          @click="restore"
          :disabled="isRestoring || !allFilled"
          class="path-action-button path-action-button--continue disabled:opacity-50"
        >
          <span v-if="isRestoring" class="flex items-center justify-center gap-2">
            <svg class="animate-spin h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
              <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
              <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
            </svg>
            Restoring...
          </span>
          <span v-else>Restore Identity</span>
        </button>
        <button
          v-else
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
const wordInputs = ref<HTMLInputElement[]>([])
const seedWords = ref<string[]>(Array(24).fill(''))
const restored = ref(false)
const isRestoring = ref(false)
const errorMessage = ref('')
const serverStarting = ref(false)
const restoredDid = ref('')
const restoredNpub = ref('')

const allFilled = computed(() => seedWords.value.every(w => w.trim().length > 0))

function goBack() {
  playNavSound('action')
  router.push('/onboarding/path').catch(() => {})
}

onMounted(() => {
  nextTick(() => {
    setTimeout(() => wordInputs.value[0]?.focus({ preventScroll: true }), 300)
  })
})

function onWordInput(index: number) {
  seedWords.value[index] = (seedWords.value[index] ?? '').trim().toLowerCase()
}

function onPaste(event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text')?.trim()
  if (!text) return

  const pastedWords = text.split(/\s+/)
  if (pastedWords.length === 24) {
    event.preventDefault()
    for (let i = 0; i < 24; i++) {
      seedWords.value[i] = (pastedWords[i] ?? '').toLowerCase()
    }
  }
}

async function restore() {
  if (!allFilled.value) return
  isRestoring.value = true
  errorMessage.value = ''
  serverStarting.value = false

  try {
    const words = seedWords.value.map(w => w.trim().toLowerCase())
    const res = await rpcClient.call<{ did: string; nostr_npub: string; restored: boolean }>({
      method: 'seed.restore',
      params: { words },
    })

    if (res.restored) {
      restored.value = true
      restoredDid.value = res.did
      restoredNpub.value = res.nostr_npub || ''
      if (res.did) localStorage.setItem('neode_did', res.did)
      if (res.nostr_npub) localStorage.setItem('neode_nostr_npub', res.nostr_npub)

      nextTick(() => {
        setTimeout(() => continueButton.value?.focus({ preventScroll: true }), 100)
      })
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    if (/502|503|504|timeout|fetch|network|Failed to fetch/i.test(msg)) {
      serverStarting.value = true
    } else {
      errorMessage.value = msg || 'Restore failed. Check your seed words and try again.'
    }
  } finally {
    isRestoring.value = false
  }
}

function proceed() {
  playNavSound('action')
  router.push('/onboarding/identity').catch(() => {})
}
</script>
