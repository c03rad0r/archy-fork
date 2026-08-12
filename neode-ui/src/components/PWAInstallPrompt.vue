<template>
  <Teleport to="body">
    <Transition name="fade">
      <div
        v-if="showInstallPrompt"
        class="fixed bottom-4 left-4 right-4 md:left-auto md:right-6 md:bottom-6 md:max-w-sm z-[9998]"
      >
        <div class="glass-card p-4 flex items-center gap-4 shadow-xl">
          <img
            src="/assets/icon/pwa-192x192-v2.png"
            alt="Archipelago"
            class="w-14 h-14 rounded-xl shrink-0"
          />
          <div class="flex-1 min-w-0">
            <p class="text-white font-medium">Install Archipelago</p>
            <p class="text-white/70 text-sm">Add to your home screen for quick access</p>
          </div>
          <div class="flex gap-2 shrink-0">
            <button
              @click="dismiss"
              class="px-3 py-2 text-sm text-white/70 hover:text-white transition-colors"
            >
              Not now
            </button>
            <button
              @click="install"
              class="px-4 py-2 glass-button glass-button-sm rounded-lg text-sm font-medium"
            >
              Install
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'

const showInstallPrompt = ref(false)
let deferredPrompt: { prompt: () => Promise<{ outcome: string }> } | null = null
const DISMISS_KEY = 'archipelago_pwa_install_dismissed'

onMounted(() => {
  // Don't show in kiosk mode, if already dismissed, or if already installed
  if (localStorage.getItem('kiosk') === 'true') return
  if (sessionStorage.getItem(DISMISS_KEY) === '1') return
  if (window.matchMedia('(display-mode: standalone)').matches) return
  if ((window.navigator as Navigator & { standalone?: boolean }).standalone) return

  const handler = (e: Event) => {
    e.preventDefault()
    deferredPrompt = e as unknown as { prompt: () => Promise<{ outcome: string }> }
    showInstallPrompt.value = true
  }

  window.addEventListener('beforeinstallprompt', handler)
  ;(window as Window & { __beforeinstallpromptHandler?: EventListener }).__beforeinstallpromptHandler = handler
})

onBeforeUnmount(() => {
  window.removeEventListener('beforeinstallprompt', (window as Window & { __beforeinstallpromptHandler?: EventListener }).__beforeinstallpromptHandler as EventListener)
})

function dismiss() {
  showInstallPrompt.value = false
  sessionStorage.setItem(DISMISS_KEY, '1')
}

async function install() {
  if (!deferredPrompt) return
  const result = await deferredPrompt.prompt()
  const outcome = result?.outcome ?? 'dismissed'
  showInstallPrompt.value = false
  deferredPrompt = null
  if (outcome === 'accepted') {
    sessionStorage.removeItem(DISMISS_KEY)
  }
}
</script>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
