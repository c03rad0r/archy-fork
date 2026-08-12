<template>
  <div
    v-if="branches.length > 1"
    class="flex items-center justify-center px-4 py-1.5 animate-fade-up-fast"
  >
    <div class="glass-button-sm min-h-[44px] md:min-h-0 md:!h-7 flex items-center gap-1.5 px-2 text-xs">
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-md hover:bg-white/10 transition-colors text-white/60 hover:text-white/90 disabled:opacity-30 disabled:cursor-default"
        :disabled="currentIndex <= 0"
        aria-label="Previous branch"
        @click="switchToBranch(currentIndex - 1)"
      >
        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <span class="text-white/70 select-none whitespace-nowrap">
        Branch {{ currentIndex + 1 }} of {{ branches.length }}
      </span>
      <button
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-md hover:bg-white/10 transition-colors text-white/60 hover:text-white/90 disabled:opacity-30 disabled:cursor-default"
        :disabled="currentIndex >= branches.length - 1"
        aria-label="Next branch"
        @click="switchToBranch(currentIndex + 1)"
      >
        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useChatStore } from '@/stores/chat'

const chatStore = useChatStore()

const branches = computed(() => {
  if (!chatStore.activeConversationId) return []
  return chatStore.getSiblingBranches(chatStore.activeConversationId)
})

const currentIndex = computed(() => {
  return branches.value.findIndex(b => b.isCurrent)
})

function switchToBranch(index: number) {
  const branch = branches.value[index]
  if (branch) {
    chatStore.setActiveConversation(branch.id)
  }
}
</script>
