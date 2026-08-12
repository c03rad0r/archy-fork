<template>
  <div class="min-h-full flex items-center justify-center p-3 sm:p-4 md:p-6">
    <!-- Main Glass Container - Scrollable -->
    <div class="max-w-[800px] w-full relative z-10 path-glass-container onb-scroll-container">
      <!-- Header -->
      <div class="text-center mb-4 sm:mb-6 flex-shrink-0 px-3 sm:px-4 pt-4 sm:pt-6">
        <h1 class="text-xl sm:text-2xl md:text-[26px] font-semibold text-white/96 mb-2 drop-shadow-[0_2px_6px_rgba(0,0,0,0.4)]">
          Backup Your Identity
        </h1>
        <p class="text-sm sm:text-base md:text-[20px] text-white/75 leading-relaxed max-w-[600px] mx-auto">
          Create a secure backup of your identity. Set a passphrase and download your encrypted backup file.
        </p>
      </div>

      <!-- Content Area -->
      <div class="flex flex-col items-center gap-4 sm:gap-6 mb-4 sm:mb-6 px-3 sm:px-4">
        <div class="w-full max-w-[600px] space-y-4 sm:space-y-6">
          <p v-if="serverStarting" class="text-orange-400/80 text-sm">Server is still starting up. Please try again shortly.</p>
          <p v-else-if="errorMessage" class="text-red-400 text-sm">{{ errorMessage }}</p>
          <!-- Passphrase Input -->
          <div class="path-option-card cursor-default px-4 py-4 sm:px-6 sm:py-6">
            <div class="text-left w-full">
              <label class="block text-xs sm:text-sm font-semibold text-white/80 mb-2 sm:mb-3 uppercase tracking-wide">
                Backup Passphrase
              </label>
              <div class="relative">
                <div class="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                  <svg class="w-5 h-5 text-white/60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
                  </svg>
                </div>
                <input
                  ref="passphraseInput"
                  v-model="passphrase"
                  type="password"
                  placeholder="Enter a strong passphrase"
                  class="w-full bg-black/40 border border-white/10 rounded-lg px-4 py-3 pl-12 text-white/95 placeholder-white/40 focus:outline-none focus:border-white/30 focus:bg-black/50 transition-all"
                />
              </div>
              <p class="text-xs sm:text-sm md:text-base text-white/60 mt-2 sm:mt-3">
                Keep this passphrase safe. You'll need it to restore your identity from backup.
              </p>
            </div>
          </div>

          <!-- Download Button -->
          <button
            @click="downloadBackup"
            :disabled="!passphrase || isDownloading"
            class="path-action-button path-action-button--continue w-full"
          >
            <span v-if="!isDownloading && !downloaded">Backup to Continue</span>
            <span v-else-if="isDownloading" class="flex items-center justify-center gap-2">
              <svg class="animate-spin h-5 w-5" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Downloading...
            </span>
            <span v-else class="flex items-center justify-center gap-2">
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
              </svg>
              Downloaded
            </span>
          </button>

          <!-- Success Message -->
          <div v-if="downloaded" class="text-center">
            <p class="text-sm text-white/70">
              Backup saved as <span class="font-mono text-white/90">archipelago-did-backup.json</span>
            </p>
          </div>
        </div>
      </div>

      <!-- Action Buttons -->
      <div class="flex justify-center max-w-[600px] mx-auto flex-shrink-0 px-3 sm:px-4 pb-4 sm:pb-6">
        <button
          ref="continueButton"
          @click="proceed"
          :disabled="!downloaded"
          class="path-action-button path-action-button--continue disabled:opacity-50"
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
import { rpcClient } from '@/api/rpc-client'
import { playNavSound } from '@/composables/useNavSounds'

const router = useRouter()
const passphraseInput = ref<HTMLInputElement | null>(null)
const continueButton = ref<HTMLButtonElement | null>(null)
const passphrase = ref('')

onMounted(() => {
  setTimeout(() => {
    passphraseInput.value?.focus({ preventScroll: true })
  }, 500)
})
const isDownloading = ref(false)
const downloaded = ref(false)
const errorMessage = ref('')
const serverStarting = ref(false)

async function downloadBackup() {
  if (!passphrase.value) return

  isDownloading.value = true
  errorMessage.value = ''

  try {
    const backupData = await rpcClient.createBackup(passphrase.value)

    const json = JSON.stringify(backupData, null, 2)
    const blob = new Blob([json], { type: 'application/json' })
    const url = URL.createObjectURL(blob)

    // Use a visible link appended to DOM for better mobile compatibility
    const a = document.createElement('a')
    a.href = url
    a.download = 'archipelago-did-backup.json'
    a.style.display = 'none'
    document.body.appendChild(a)
    a.click()
    // Delay cleanup so mobile browsers can start the download
    setTimeout(() => {
      document.body.removeChild(a)
      URL.revokeObjectURL(url)
    }, 1000)

    downloaded.value = true
    localStorage.setItem('neode_backup_created', '1')
    // Focus Continue button after backup completes
    setTimeout(() => {
      continueButton.value?.focus({ preventScroll: true })
    }, 100)
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err)
    if (/502|503|504|timeout|fetch|network|Failed to fetch/i.test(msg)) {
      serverStarting.value = true
    } else {
      errorMessage.value = msg || 'Failed to create backup. Please try again.'
    }
  } finally {
    isDownloading.value = false
  }
}

function proceed() {
  playNavSound('action')
  router.push('/onboarding/verify').catch(() => {})
}

</script>

