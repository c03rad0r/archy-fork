<template>
  <div class="h-dvh flex flex-col" :class="currentTheme" :style="rootStyle">
    <RouterView />
    <ArticleOverlay />
    <VideoPlayerOverlay />
    <PlayerBar v-if="!isMobile" />
    <PassphraseDialog
      :visible="showPassphrase"
      :is-creating="isCreatingPassphrase"
      :error="passphraseError"
      @submit="handlePassphraseSubmit"
      @skip="showPassphrase = false"
    />
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { RouterView } from 'vue-router'
import { useTheme } from '@/composables/useTheme'
import { useArchy } from '@/composables/useArchy'
import { useVisualViewport } from '@/composables/useVisualViewport'
import ArticleOverlay from '@/components/content/ArticleOverlay.vue'
import VideoPlayerOverlay from '@/components/player/VideoPlayerOverlay.vue'
import PlayerBar from '@/components/player/PlayerBar.vue'
import PassphraseDialog from '@/components/ui/PassphraseDialog.vue'
import {
  isCryptoEnabled,
  deriveKey,
  generateSalt,
  setSessionKey,
} from '@/utils/crypto'
import { useSettingsStore } from '@/stores/settings'

const { currentTheme, initTheme, setTheme } = useTheme()
const archy = useArchy()

// Captured before mount (main.ts), independent of the archyBridge handshake —
// forcing dark theme must not wait on any postMessage round trip completing.
const isEmbeddedFlag = !!(window as unknown as Record<string, unknown>).__AIUI_EMBEDDED__

const windowWidth = ref(window.innerWidth)
const isMobile = computed(() => windowWidth.value < 1024)
const { viewportHeight, isKeyboardOpen } = useVisualViewport()
function onResize() { windowWidth.value = window.innerWidth }

// On mobile, always bind height to visualViewport so the container
// respects the actual visible area (dvh doesn't reliably exclude
// Safari's bottom toolbar when body is position:fixed)
const rootStyle = computed(() => {
  if (isMobile.value && viewportHeight.value > 0) {
    return { height: `${viewportHeight.value}px`, overflow: 'hidden' }
  }
  return {}
})

const showPassphrase = ref(false)
const isCreatingPassphrase = ref(false)

const SALT_KEY = 'aiui-crypto-salt'

const passphraseError = ref('')

async function handlePassphraseSubmit(passphrase: string) {
  passphraseError.value = ''
  try {
    let saltHex = localStorage.getItem(SALT_KEY)
    let salt: Uint8Array

    if (saltHex) {
      salt = new Uint8Array(saltHex.match(/.{2}/g)!.map(b => parseInt(b, 16)))
    } else {
      salt = await generateSalt()
      saltHex = Array.from(salt).map(b => b.toString(16).padStart(2, '0')).join('')
      localStorage.setItem(SALT_KEY, saltHex)
    }

    const key = await deriveKey(passphrase, salt)
    setSessionKey(key, salt)
    // Now that a session key exists: pull the Claude key from the encrypted
    // vault, or migrate a just-scrubbed legacy plaintext key INTO the vault.
    await useSettingsStore().initClaudeKey()
    showPassphrase.value = false
  } catch (err) {
    console.error('[AIUI] Passphrase error:', err)
    passphraseError.value = 'Encryption failed — check your passphrase or try skipping'
  }
}

onMounted(() => {
  initTheme()

  // Archy is always dark-themed and AIUI's embedded chat is meant to match it
  // exactly — never the browser/OS light-mode default or a stale
  // localStorage('aiui-theme', 'light') from a prior standalone visit. This
  // must not depend on the archyBridge 'ready'/theme handshake completing
  // (that round trip can be slow or fail outright), so it runs unconditionally
  // right after initTheme(), overriding whatever it just decided.
  if (isEmbeddedFlag) setTheme('dark')

  window.addEventListener('resize', onResize)

  // Initialize Archy bridge when running embedded in Archipelago
  archy.init()

  // Skip encryption prompt when embedded in Archy — Archy handles auth
  if (isCryptoEnabled() && !archy.isEmbedded.value) {
    const hasSalt = !!localStorage.getItem(SALT_KEY)
    isCreatingPassphrase.value = !hasSalt
    showPassphrase.value = true
  }
})

onUnmounted(() => {
  window.removeEventListener('resize', onResize)
  archy.destroy()
})
</script>
