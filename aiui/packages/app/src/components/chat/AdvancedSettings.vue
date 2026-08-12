<template>
  <div class="px-3 md:px-4 pb-1">
    <button
      class="flex items-center gap-1.5 text-xs text-white/40 hover:text-white/60 transition-colors"
      @click="isExpanded = !isExpanded"
    >
      <svg
        class="w-3 h-3 transition-transform duration-200"
        :class="isExpanded ? 'rotate-90' : ''"
        fill="currentColor"
        viewBox="0 0 20 20"
      >
        <path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd" />
      </svg>
      Advanced
    </button>

    <div v-if="isExpanded && conv" class="mt-2 space-y-3 animate-fade-up-fast">
      <!-- Temperature -->
      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <label class="text-xs text-white/40">Temperature</label>
          <span class="text-xs text-white/50 tabular-nums">{{ temperature.toFixed(2) }}</span>
        </div>
        <input
          v-model.number="temperature"
          type="range"
          min="0"
          max="1"
          step="0.05"
          class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
          @input="persistParams"
        />
      </div>

      <!-- Max Tokens -->
      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <label class="text-xs text-white/40">Max Tokens</label>
          <span class="text-xs text-white/50 tabular-nums">{{ maxTokens }}</span>
        </div>
        <input
          v-model.number="maxTokens"
          type="range"
          min="256"
          max="8192"
          step="256"
          class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
          @input="persistParams"
        />
      </div>

      <!-- Top P -->
      <div class="space-y-1">
        <div class="flex items-center justify-between">
          <label class="text-xs text-white/40">Top P</label>
          <span class="text-xs text-white/50 tabular-nums">{{ topP.toFixed(2) }}</span>
        </div>
        <input
          v-model.number="topP"
          type="range"
          min="0"
          max="1"
          step="0.05"
          class="w-full h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
          @input="persistParams"
        />
      </div>

      <!-- Stop Sequences (M9.10) -->
      <div class="space-y-1">
        <label class="text-xs text-white/40">Stop Sequences</label>
        <div v-if="stopSequences.length > 0" class="flex gap-1 flex-wrap mb-1">
          <span
            v-for="(seq, i) in stopSequences"
            :key="i"
            class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-xs text-white/50"
          >
            {{ seq }}
            <button class="text-white/30 hover:text-white/60" @click="removeStopSequence(i)">×</button>
          </span>
        </div>
        <input
          v-model="newStopSeq"
          type="text"
          class="w-full bg-white/5 border border-white/10 rounded-lg px-2.5 py-1.5 text-base text-white/80 outline-none focus:border-accent/40 placeholder:text-white/25"
          placeholder="Add stop sequence (Enter to add)"
          @keydown.enter="addStopSequence"
        />
      </div>

      <!-- Reset -->
      <button
        class="text-xs text-white/30 hover:text-white/50 transition-colors"
        @click="resetDefaults"
      >
        Reset to defaults
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useChatStore } from '@/stores/chat'

const chatStore = useChatStore()

const isExpanded = ref(false)
const newStopSeq = ref('')

const conv = computed(() => chatStore.activeConversation)

const temperature = ref(1.0)
const maxTokens = ref(4096)
const topP = ref(1.0)
const stopSequences = ref<string[]>([])

// Sync from conversation on switch
watch(
  () => chatStore.activeConversationId,
  () => loadFromConv(),
  { immediate: true }
)

function loadFromConv() {
  const c = conv.value
  temperature.value = c?.temperature ?? 1.0
  maxTokens.value = c?.maxTokens ?? 4096
  topP.value = c?.topP ?? 1.0
  stopSequences.value = c?.stopSequences ? [...c.stopSequences] : []
}

function persistParams() {
  const c = conv.value
  if (!c) return
  c.temperature = temperature.value
  c.maxTokens = maxTokens.value
  c.topP = topP.value
  c.updatedAt = Date.now()
}

function addStopSequence() {
  const seq = newStopSeq.value.trim()
  if (!seq) return
  stopSequences.value.push(seq)
  newStopSeq.value = ''
  const c = conv.value
  if (c) {
    c.stopSequences = [...stopSequences.value]
    c.updatedAt = Date.now()
  }
}

function removeStopSequence(index: number) {
  stopSequences.value.splice(index, 1)
  const c = conv.value
  if (c) {
    c.stopSequences = [...stopSequences.value]
    c.updatedAt = Date.now()
  }
}

function resetDefaults() {
  temperature.value = 1.0
  maxTokens.value = 4096
  topP.value = 1.0
  stopSequences.value = []
  newStopSeq.value = ''
  const c = conv.value
  if (c) {
    c.temperature = undefined
    c.maxTokens = undefined
    c.topP = undefined
    c.stopSequences = undefined
    c.updatedAt = Date.now()
  }
}
</script>
