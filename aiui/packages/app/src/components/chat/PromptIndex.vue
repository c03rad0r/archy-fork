<template>
  <div class="flex-1 min-h-0 overflow-y-auto scrollbar-hide p-3 space-y-1">
    <div v-if="promptPairs.length === 0" class="flex items-center justify-center h-full">
      <p class="text-xs text-white/30">
        No prompts yet
      </p>
    </div>

    <button
      v-for="(pair, i) in promptPairs"
      :key="pair.userMsg.id"
      class="w-full text-left px-3 py-2.5 rounded-xl transition-all duration-150 group"
      :class="[
        activeIndex === i
          ? 'path-glass-bubble-user'
          : 'hover:bg-white/5'
      ]"
      @click="selectPrompt(pair, i)"
    >
      <p class="text-sm leading-snug truncate text-white/90">
        {{ pair.userMsg.content }}
      </p>
      <div class="flex items-center gap-1.5 mt-1 flex-wrap">
        <span class="text-xs text-white/30">
          {{ formatTime(pair.userMsg.timestamp) }}
        </span>
        <span
          v-for="badge in pair.badges"
          :key="badge"
          class="text-xs px-1.5 py-0.5 rounded-md bg-white/8 text-white/40"
        >
          {{ badge }}
        </span>
      </div>
    </button>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { Message } from '@aiui/core/types/message'
import { useContentPanel } from '@/composables/useContentPanel'

interface PromptPair {
  userMsg: Message
  assistantMsg: Message | null
  badges: string[]
}

const props = defineProps<{
  messages: Message[]
}>()

const emit = defineEmits<{
  select: [userMsg: Message, assistantMsg: Message | null]
}>()

const { getContextualInlineContent } = useContentPanel()
const activeIndex = ref<number | null>(null)

const promptPairs = computed<PromptPair[]>(() => {
  const pairs: PromptPair[] = []
  const msgs = props.messages

  for (let i = 0; i < msgs.length; i++) {
    if (msgs[i].role !== 'user') continue

    const userMsg = msgs[i]
    const assistantMsg = (i + 1 < msgs.length && msgs[i + 1].role === 'assistant')
      ? msgs[i + 1]
      : null

    const badges: string[] = []
    if (assistantMsg && assistantMsg.content) {
      const content = getContextualInlineContent(
        assistantMsg.content,
        userMsg.content,
        assistantMsg.webResults ?? [],
      )
      if (content.films.length > 0) badges.push('Films')
      if ((content.books?.length ?? 0) > 0) badges.push('Books')
      if ((content.tvSeries?.length ?? 0) > 0) badges.push('TV')
      if ((content.images?.length ?? 0) > 0) badges.push('Images')
      if ((content.places?.length ?? 0) > 0) badges.push('Places')
      if (content.songs.length > 0) badges.push('Music')
      if (content.podcasts.length > 0) badges.push('Podcasts')
      if (content.magazineSections.length > 0) badges.push('Magazine')
      if ((content.newsLinks?.length ?? 0) > 0) badges.push('News')
      if ((content.websitesLinks?.length ?? 0) > 0) badges.push('Web')
      if ((content.codeBlocks?.length ?? 0) > 0) badges.push('Code')
      if ((content.apps?.length ?? 0) > 0) badges.push('Apps')
      if (content.hasNostr) badges.push('Nostr')
    }

    pairs.push({ userMsg, assistantMsg, badges })
  }

  return pairs
})

function selectPrompt(pair: PromptPair, index: number) {
  activeIndex.value = index
  emit('select', pair.userMsg, pair.assistantMsg)
}

function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
</script>
