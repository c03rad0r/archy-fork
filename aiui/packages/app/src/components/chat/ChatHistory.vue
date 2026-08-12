<template>
  <div class="flex-1 min-h-0 overflow-y-auto scrollbar-hide p-3 space-y-1">
    <button
      class="w-full text-left px-3 py-2.5 min-h-[44px] rounded-xl transition-all duration-150 hover:bg-white/5 flex items-center gap-2 text-white/70 mb-2"
      @click="$emit('newChat')"
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
      </svg>
      <span class="text-sm font-medium">New Chat</span>
    </button>

    <div v-if="conversations.length === 0" class="flex items-center justify-center h-32">
      <p class="text-xs text-white/30">No conversations yet</p>
    </div>

    <button
      v-for="conv in conversations"
      :key="conv.id"
      class="w-full text-left px-3 py-2.5 min-h-[44px] rounded-xl transition-all duration-150"
      :class="conv.id === activeId ? 'nav-tab-active' : 'hover:bg-white/5'"
      @click="selectConversation(conv.id)"
    >
      <p class="text-sm leading-snug truncate" :class="conv.id === activeId ? 'text-white' : 'text-white/90'">
        {{ conv.title || 'Untitled' }}
      </p>
      <div class="flex items-center gap-1.5 mt-1">
        <span class="text-xs text-white/30">
          {{ formatTime(conv.updatedAt) }}
        </span>
        <span class="text-xs text-white/20">&middot;</span>
        <span class="text-xs text-white/30">
          {{ conv.messages.length }} msg{{ conv.messages.length !== 1 ? 's' : '' }}
        </span>
      </div>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useChatStore } from '@/stores/chat'

const emit = defineEmits<{
  select: [id: string]
  newChat: []
}>()

const chatStore = useChatStore()

const conversations = computed(() => chatStore.conversationList)
const activeId = computed(() => chatStore.activeConversationId)

function selectConversation(id: string) {
  emit('select', id)
}

function formatTime(ts: number): string {
  const now = Date.now()
  const diff = now - ts
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'Just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 7) return `${days}d ago`
  return new Date(ts).toLocaleDateString([], { month: 'short', day: 'numeric' })
}
</script>
