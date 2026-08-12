<template>
  <div class="min-h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <!-- Main Glass Container -->
    <div class="max-w-[800px] w-full relative z-10 path-glass-container onb-scroll-container">
      <!-- Header (before DID is retrieved) -->
      <div v-if="!generatedDid" class="text-center flex-shrink-0">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-3 sm:mb-6 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Your node's identity
        </h1>
        <p class="text-sm sm:text-base md:text-[20px] text-white/75 leading-relaxed max-w-[600px] mx-auto mb-4 sm:mb-6">
          Your node has a Decentralized Identifier (DID) for secure, passwordless authentication.
        </p>
      </div>

      <!-- Content Area -->
      <div class="flex flex-col items-center gap-6 mb-6">
        <!-- Waiting for server / Generating state -->
        <div v-if="!generatedDid && (isGenerating || waitingForServer)" class="text-center">
          <div class="flex justify-center mb-4">
            <div class="w-16 h-16 rounded-full bg-white/10 flex items-center justify-center onb-lock-spin">
              <svg class="w-8 h-8 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
              </svg>
            </div>
          </div>
          <div v-if="waitingForServer" class="flex items-center justify-center gap-3 mb-2">
            <p class="text-lg text-white/80">Server starting up</p>
            <span class="text-sm text-white/40 font-mono tabular-nums">{{ elapsedDisplay }}</span>
          </div>
          <p v-if="waitingForServer" class="text-sm text-white/50">This usually takes 1–3 minutes after first boot</p>
          <p v-if="!waitingForServer" class="text-lg text-white/80">Generating your identity key...</p>
        </div>

        <!-- Generated DID Display -->
        <div v-if="generatedDid" class="w-full max-w-[600px] space-y-4">
          <!-- Success Message -->
          <div class="text-center mb-6">
            <div class="flex justify-center mb-6">
              <div class="path-option-card cursor-default w-20 h-20 rounded-full flex items-center justify-center">
                <svg class="w-10 h-10 text-black" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="3">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                </svg>
              </div>
            </div>
            <p class="text-[20px] text-white/80 leading-relaxed max-w-[600px] mx-auto mb-6">
              Your node's decentralized identifier
            </p>
          </div>

          <!-- DID Display Card -->
          <div class="path-option-card cursor-default px-6 py-6">
            <div class="text-left">
              <h3 class="text-sm font-semibold text-white/80 mb-2 uppercase tracking-wide">Your DID</h3>
              <div class="bg-black/40 rounded-lg p-4 mb-3 backdrop-blur-sm border border-white/10 flex items-start gap-3">
                <p class="text-white/95 font-mono text-sm break-all leading-relaxed flex-1">
                  {{ generatedDid }}
                </p>
                <button
                  @click="copyDid"
                  class="shrink-0 p-1.5 rounded hover:bg-white/10 transition-colors text-white/50 hover:text-white/90"
                  :title="didCopied ? 'Copied!' : 'Copy DID'"
                >
                  <svg v-if="!didCopied" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                  </svg>
                  <svg v-else class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
              </div>
              <p class="text-xs text-white/50 mb-3">For Web5, federation, and verifiable credentials</p>
            </div>

            <!-- Nostr ID -->
            <div v-if="nostrNpub" class="text-left mt-4">
              <h3 class="text-sm font-semibold text-white/80 mb-2 uppercase tracking-wide">Your Nostr ID</h3>
              <div class="bg-black/40 rounded-lg p-4 mb-3 backdrop-blur-sm border border-white/10 flex items-start gap-3">
                <p class="text-white/95 font-mono text-sm break-all leading-relaxed flex-1">
                  {{ nostrNpub }}
                </p>
                <button
                  @click="copyNpub"
                  class="shrink-0 p-1.5 rounded hover:bg-white/10 transition-colors text-white/50 hover:text-white/90"
                  :title="npubCopied ? 'Copied!' : 'Copy npub'"
                >
                  <svg v-if="!npubCopied" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                  </svg>
                  <svg v-else class="w-4 h-4 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7" />
                  </svg>
                </button>
              </div>
              <p class="text-xs text-white/50">For Nostr social apps and NIP-07 signing</p>
            </div>
          </div>
        </div>
      </div>

      <!-- Action Buttons -->
      <div class="flex justify-center max-w-[600px] mx-auto flex-shrink-0">
        <button
          v-if="generatedDid"
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
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { rpcClient } from '@/api/rpc-client'

const router = useRouter()
const continueButton = ref<HTMLButtonElement | null>(null)
const generatedDid = ref<string>('')
const nostrNpub = ref<string>('')
const isGenerating = ref(false)
const waitingForServer = ref(false)
const didCopied = ref(false)
const npubCopied = ref(false)
const elapsedDisplay = ref('0:00')
let retryTimer: ReturnType<typeof setTimeout> | null = null
let elapsedTimer: ReturnType<typeof setInterval> | null = null
let startTime = 0

function startElapsedTimer() {
  startTime = Date.now()
  elapsedTimer = setInterval(() => {
    const secs = Math.floor((Date.now() - startTime) / 1000)
    const m = Math.floor(secs / 60)
    const s = secs % 60
    elapsedDisplay.value = `${m}:${s.toString().padStart(2, '0')}`
  }, 1000)
}

function stopTimers() {
  if (retryTimer) { clearTimeout(retryTimer); retryTimer = null }
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null }
}

function storeDidState(did: string, pubkey: string) {
  localStorage.setItem('neode_did', did)
  localStorage.setItem('neode_did_state', JSON.stringify({ did, kid: `${did}#key-1`, pubkey }))
}

async function fetchDid() {
  if (!waitingForServer.value) {
    isGenerating.value = true
  }

  try {
    const { did, pubkey } = await rpcClient.getNodeDid()
    stopTimers()
    generatedDid.value = did
    storeDidState(did, pubkey)
    isGenerating.value = false
    waitingForServer.value = false

    // Fetch Nostr npub in parallel (non-blocking)
    rpcClient.getNostrPubkey().then(({ nostr_npub }) => {
      if (nostr_npub) {
        nostrNpub.value = nostr_npub
        localStorage.setItem('neode_nostr_npub', nostr_npub)
      }
    }).catch(() => { /* Nostr key may not exist yet */ })

  } catch {
    isGenerating.value = false
    if (!waitingForServer.value) {
      waitingForServer.value = true
      startElapsedTimer()
    }
    retryTimer = setTimeout(fetchDid, 4000)
  }
}

watch(generatedDid, (did) => {
  if (did) {
    nextTick(() => {
      setTimeout(() => {
        continueButton.value?.focus({ preventScroll: true })
      }, 100)
    })
  }
})

onMounted(() => {
  const cached = localStorage.getItem('neode_did')
  const cachedNpub = localStorage.getItem('neode_nostr_npub')
  if (cachedNpub) nostrNpub.value = cachedNpub
  if (cached && !cached.includes('...')) {
    generatedDid.value = cached
  } else {
    fetchDid()
  }
})

onUnmounted(() => {
  stopTimers()
})

function proceed() {
  stopTimers()
  router.push('/onboarding/identity').catch(() => {})
}

function copyDid() {
  if (!generatedDid.value) return
  navigator.clipboard.writeText(generatedDid.value).catch(() => {})
  didCopied.value = true
  setTimeout(() => { didCopied.value = false }, 2000)
}

function copyNpub() {
  if (!nostrNpub.value) return
  navigator.clipboard.writeText(nostrNpub.value).catch(() => {})
  npubCopied.value = true
  setTimeout(() => { npubCopied.value = false }, 2000)
}
</script>

<style scoped>
.onb-lock-spin {
  animation: onb-lock-pulse 1.2s ease-in-out infinite;
}
@keyframes onb-lock-pulse {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(1.08); opacity: 0.7; }
}
</style>
