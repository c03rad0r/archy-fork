<template>
  <div
    v-if="isOpen"
    class="glass px-3 py-2 mx-3 mb-1 rounded-xl flex items-center gap-2 animate-fade-up-fast"
  >
    <svg class="w-4 h-4 text-white/40 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
    </svg>
    <input
      ref="inputRef"
      v-model="query"
      type="text"
      placeholder="Search messages..."
      class="flex-1 bg-transparent text-base text-white/90 placeholder:text-white/25 outline-none min-w-0"
      @keydown.enter.exact="nextMatch"
      @keydown.shift.enter="prevMatch"
      @keydown.escape="close"
      @keydown.up.prevent="prevMatch"
      @keydown.down.prevent="nextMatch"
    />
    <span v-if="query" class="text-xs text-white/40 whitespace-nowrap select-none">
      {{ matchCount > 0 ? `${currentMatchIndex + 1}/${matchCount}` : 'No results' }}
    </span>
    <button
      class="touch-target rounded-md hover:bg-white/10 transition-colors text-white/50 hover:text-white/80 disabled:opacity-30"
      :disabled="matchCount === 0"
      aria-label="Previous match"
      @click="prevMatch"
    >
      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 15l7-7 7 7" />
      </svg>
    </button>
    <button
      class="touch-target rounded-md hover:bg-white/10 transition-colors text-white/50 hover:text-white/80 disabled:opacity-30"
      :disabled="matchCount === 0"
      aria-label="Next match"
      @click="nextMatch"
    >
      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>
    <button
      class="touch-target rounded-md hover:bg-white/10 transition-colors text-white/50 hover:text-white/80"
      aria-label="Close search"
      @click="close"
    >
      <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch, nextTick, onMounted, onUnmounted } from 'vue'
import type { Message } from '@aiui/core/types/message'

const props = defineProps<{
  messages: Message[]
}>()

const emit = defineEmits<{
  scrollToMessage: [index: number]
}>()

const isOpen = ref(false)
const query = ref('')
const currentMatchIndex = ref(0)
const inputRef = ref<HTMLInputElement | null>(null)

const matchingIndices = computed(() => {
  if (!query.value.trim()) return []
  const q = query.value.toLowerCase()
  const indices: number[] = []
  for (let i = 0; i < props.messages.length; i++) {
    if (props.messages[i].content.toLowerCase().includes(q)) {
      indices.push(i)
    }
  }
  return indices
})

const matchCount = computed(() => matchingIndices.value.length)

watch(query, () => {
  currentMatchIndex.value = 0
  if (matchingIndices.value.length > 0) {
    emit('scrollToMessage', matchingIndices.value[0])
  }
})

function nextMatch() {
  if (matchCount.value === 0) return
  currentMatchIndex.value = (currentMatchIndex.value + 1) % matchCount.value
  emit('scrollToMessage', matchingIndices.value[currentMatchIndex.value])
}

function prevMatch() {
  if (matchCount.value === 0) return
  currentMatchIndex.value = (currentMatchIndex.value - 1 + matchCount.value) % matchCount.value
  emit('scrollToMessage', matchingIndices.value[currentMatchIndex.value])
}

function open() {
  isOpen.value = true
  nextTick(() => inputRef.value?.focus())
}

function close() {
  isOpen.value = false
  query.value = ''
  currentMatchIndex.value = 0
}

function handleKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
    e.preventDefault()
    if (isOpen.value) {
      inputRef.value?.focus()
    } else {
      open()
    }
  }
}

onMounted(() => {
  document.addEventListener('keydown', handleKeydown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', handleKeydown)
})

defineExpose({ open, close, isOpen, matchingIndices, currentMatchIndex })
</script>
