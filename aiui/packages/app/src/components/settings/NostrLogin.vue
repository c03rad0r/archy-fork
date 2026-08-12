<template>
  <div class="space-y-3">
    <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
      Nostr Identity
    </h3>

    <!-- Logged in state -->
    <div
      v-if="isLoggedIn"
      class="flex items-center gap-3 p-3 rounded-xl"
      :class="isDark
        ? 'bg-white/[0.03] border border-white/5'
        : 'bg-black/[0.02] border border-black/5'"
    >
      <div class="w-8 h-8 rounded-full bg-purple-500/20 flex items-center justify-center">
        <svg class="w-4 h-4 text-purple-400" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 3c1.66 0 3 1.34 3 3s-1.34 3-3 3-3-1.34-3-3 1.34-3 3-3zm0 14.2c-2.5 0-4.71-1.28-6-3.22.03-1.99 4-3.08 6-3.08 1.99 0 5.97 1.09 6 3.08-1.29 1.94-3.5 3.22-6 3.22z" />
        </svg>
      </div>
      <div class="flex-1 min-w-0">
        <div
          class="text-xs font-medium"
          :class="isDark ? 'text-white/80' : 'text-gray-800'"
        >
          Connected
        </div>
        <button
          class="text-xs font-mono mt-0.5 hover:underline"
          :class="isDark ? 'text-purple-400/60' : 'text-purple-600/60'"
          @click="copyNpub"
        >
          {{ copied ? 'Copied!' : truncatedNpub }}
        </button>
      </div>
      <button
        class="text-xs px-2 py-1 rounded-md transition-colors"
        :class="isDark
          ? 'text-red-400/60 hover:text-red-400 hover:bg-red-500/10'
          : 'text-red-500/60 hover:text-red-600 hover:bg-red-50'"
        @click="logout"
      >
        Logout
      </button>
    </div>

    <!-- Logged out state -->
    <template v-else>
      <button
        v-if="isAvailable"
        :disabled="isLoading"
        class="w-full px-4 py-2.5 rounded-xl text-sm font-medium transition-all active:scale-[0.98]"
        :class="isLoading
          ? isDark
            ? 'bg-purple-500/10 text-purple-400/40 cursor-wait'
            : 'bg-purple-50 text-purple-300 cursor-wait'
          : 'bg-purple-500/10 text-purple-400 hover:bg-purple-500/20'"
        @click="login"
      >
        {{ isLoading ? 'Connecting...' : 'Login with Nostr' }}
      </button>

      <div
        v-else
        class="p-3 rounded-xl text-xs"
        :class="isDark
          ? 'bg-white/[0.03] border border-white/5 text-white/40'
          : 'bg-black/[0.02] border border-black/5 text-gray-500'"
      >
        No Nostr extension detected. Install
        <a
          href="https://github.com/nicolgit/nos2x"
          target="_blank"
          rel="noopener noreferrer"
          class="text-purple-400 hover:underline"
        >nos2x</a>,
        <a
          href="https://getalby.com"
          target="_blank"
          rel="noopener noreferrer"
          class="text-purple-400 hover:underline"
        >Alby</a>, or another NIP-07 browser extension.
      </div>
    </template>

    <!-- Error -->
    <p
      v-if="error"
      class="text-xs text-red-400"
    >
      {{ error }}
    </p>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useNostrIdentity } from '@/composables/useNostrIdentity'

const { isDark } = useTheme()
const {
  isAvailable,
  isLoggedIn,
  isLoading,
  error,
  npub,
  truncatedNpub,
  login,
  logout,
} = useNostrIdentity()

const copied = ref(false)

async function copyNpub() {
  if (!npub.value) return
  await navigator.clipboard.writeText(npub.value)
  copied.value = true
  setTimeout(() => { copied.value = false }, 2000)
}
</script>
