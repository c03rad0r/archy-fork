<template>
  <!-- Compact mode: small pill for chat fullscreen -->
  <div v-if="compact" class="chat-mode-pill-inner" @click="handleCompactClick">
    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
    </svg>
    <span class="text-xs font-medium">{{ currentLabel }}</span>
  </div>

  <!-- Full mode switcher -->
  <div v-else class="mode-switcher mode-switcher-full">
    <button
      v-for="m in modes"
      :key="m.id"
      @click="uiMode.setMode(m.id)"
      class="mode-switcher-btn"
      :class="{ 'mode-switcher-btn-active': uiMode.mode === m.id }"
    >
      {{ m.label }}
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useUIModeStore } from '@/stores/uiMode'
import type { UIMode } from '@/types/api'

const props = defineProps<{
  compact?: boolean
}>()

const uiMode = useUIModeStore()
const router = useRouter()

const modes: { id: UIMode; label: string }[] = [
  { id: 'easy', label: 'Easy' },
  { id: 'gamer', label: 'Pro' },
]

const currentLabel = computed(() => {
  const found = modes.find(m => m.id === uiMode.mode)
  return found ? found.label : 'Pro'
})

function handleCompactClick() {
  const newMode = uiMode.cycleMode()
  router.push(newMode === 'chat' ? '/dashboard/chat' : '/dashboard')
}
</script>
