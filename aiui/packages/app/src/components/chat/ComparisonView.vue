<template>
  <div class="flex flex-col h-full">
    <!-- Mobile tabs -->
    <div class="flex md:hidden gap-2 px-3 pt-2">
      <button
        v-for="(tab, i) in tabs"
        :key="i"
        class="flex-1 text-sm min-h-[44px] rounded-lg transition-all"
        :class="activeTab === i
          ? 'bg-accent/20 text-accent'
          : 'text-white/50 hover:text-white/70'"
        @click="activeTab = i"
      >
        {{ tab }}
      </button>
    </div>

    <!-- Split panes -->
    <div class="flex-1 min-h-0 flex gap-0.5 p-2">
      <!-- Model 1 -->
      <div
        class="flex-1 min-w-0 flex flex-col glass rounded-xl overflow-hidden"
        :class="{ 'hidden md:flex': activeTab !== 0 }"
      >
        <div class="px-3 py-2 flex items-center justify-between border-b border-white/5">
          <span class="text-xs text-accent font-medium truncate">{{ model1Label }}</span>
          <span v-if="isStreaming1" class="text-xs text-white/30 animate-pulse">streaming...</span>
        </div>
        <div class="flex-1 overflow-y-auto p-3">
          <div
            v-if="response1"
            class="chat-markdown text-sm leading-relaxed break-words text-white/90"
            v-html="rendered1"
          />
          <div v-else-if="error1" class="text-sm text-red-400/80">{{ error1 }}</div>
          <div v-else class="text-sm text-white/25 italic">Waiting for response...</div>
        </div>
      </div>

      <!-- Model 2 -->
      <div
        class="flex-1 min-w-0 flex flex-col glass rounded-xl overflow-hidden"
        :class="{ 'hidden md:flex': activeTab !== 1 }"
      >
        <div class="px-3 py-2 flex items-center justify-between border-b border-white/5">
          <span class="text-xs text-accent font-medium truncate">{{ model2Label }}</span>
          <span v-if="isStreaming2" class="text-xs text-white/30 animate-pulse">streaming...</span>
        </div>
        <div class="flex-1 overflow-y-auto p-3">
          <div
            v-if="response2"
            class="chat-markdown text-sm leading-relaxed break-words text-white/90"
            v-html="rendered2"
          />
          <div v-else-if="error2" class="text-sm text-red-400/80">{{ error2 }}</div>
          <div v-else class="text-sm text-white/25 italic">Waiting for response...</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import MarkdownIt from 'markdown-it'
import { useComparisonMode } from '@/composables/useComparisonMode'

const { model1, model2, response1, response2, isStreaming1, isStreaming2, error1, error2 } = useComparisonMode()

const activeTab = ref(0)
const tabs = computed(() => [model1Label.value, model2Label.value])

const model1Label = computed(() => `${model1.value.provider}/${model1.value.model}`.split('/').pop() ?? 'Model 1')
const model2Label = computed(() => `${model2.value.provider}/${model2.value.model}`.split('/').pop() ?? 'Model 2')

const md = new MarkdownIt({ html: false, linkify: true, breaks: true })
const rendered1 = computed(() => md.render(response1.value))
const rendered2 = computed(() => md.render(response2.value))
</script>
