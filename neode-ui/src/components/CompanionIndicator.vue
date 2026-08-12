<template>
  <Transition name="companion-slide">
    <div
      v-if="relayConnected"
      class="flex items-center gap-2 px-3 py-2 rounded-lg transition-all duration-300"
      :class="companionActive
        ? 'bg-orange-500/10 border border-orange-500/20'
        : 'bg-white/5 border border-white/10'"
      :title="companionActive ? 'Companion app connected' : 'Remote relay ready'"
    >
      <svg
        class="w-5 h-5 flex-shrink-0 transition-colors duration-200"
        :class="companionActive
          ? (companionInputActive ? 'text-orange-400' : 'text-orange-400/70')
          : 'text-white/20'"
        fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"
      >
        <rect x="3" y="7" width="18" height="11" rx="3" stroke-width="1.5" />
        <rect x="7.5" y="10" width="2" height="5" rx="0.5" fill="currentColor" />
        <rect x="6" y="11.5" width="5" height="2" rx="0.5" fill="currentColor" />
        <circle cx="16" cy="11" r="1.2" fill="currentColor" />
        <circle cx="14" cy="13.5" r="1.2" fill="currentColor" />
      </svg>
      <span
        class="text-xs hidden sm:inline transition-colors duration-200"
        :class="companionActive ? 'text-orange-400/80' : 'text-white/30'"
      >{{ companionActive ? 'Companion' : 'Relay' }}</span>
      <span
        v-if="companionInputActive"
        class="ml-auto w-1.5 h-1.5 rounded-full bg-orange-400 animate-pulse"
      />
    </div>
  </Transition>
</template>

<script setup lang="ts">
import { relayConnected, companionActive, companionInputActive } from '@/api/remote-relay'
</script>

<style scoped>
.companion-slide-enter-active { transition: opacity 0.3s ease, max-height 0.3s ease; }
.companion-slide-leave-active { transition: opacity 0.2s ease, max-height 0.2s ease; }
.companion-slide-enter-from,
.companion-slide-leave-to { opacity: 0; max-height: 0; }
</style>
