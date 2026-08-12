<template>
  <div
    v-if="messages.length > 0"
    class="h-1 mx-3 rounded-full bg-white/5 overflow-hidden shrink-0 group cursor-help relative"
    :title="tooltipText"
  >
    <div
      class="h-full rounded-full transition-all duration-500"
      :class="percentage > 80 ? 'bg-red-500' : 'bg-accent'"
      :style="{ width: `${Math.min(percentage, 100)}%` }"
    />
    <!-- Tooltip on hover -->
    <div
      class="absolute -top-8 left-1/2 -translate-x-1/2 hidden group-hover:flex items-center px-2 py-1 rounded-md bg-black/80 text-xs text-white/80 whitespace-nowrap pointer-events-none z-10"
    >
      {{ tooltipText }}
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { Message } from '@aiui/core/types/message'

const props = defineProps<{
  messages: Message[]
  contextWindow?: number
  activeModel?: string
}>()

// Default context window sizes per model (tokens)
const maxTokens = computed(() => props.contextWindow ?? 200000)

// Estimate: ~4 chars per token
const estimatedTokens = computed(() => {
  let chars = 0
  for (const msg of props.messages) {
    chars += msg.content.length
  }
  return Math.ceil(chars / 4)
})

const percentage = computed(() => {
  if (maxTokens.value === 0) return 0
  return (estimatedTokens.value / maxTokens.value) * 100
})

// Model pricing per million tokens (input/output)
const MODEL_PRICING: Record<string, { input: number; output: number }> = {
  'claude-haiku-4.5': { input: 0.80, output: 4.00 },
  'claude-sonnet-4': { input: 3.00, output: 15.00 },
  'claude-opus-4': { input: 15.00, output: 75.00 },
}

const estimatedCost = computed(() => {
  const pricing = MODEL_PRICING[props.activeModel ?? '']
  if (!pricing) return null
  // Rough split: 70% input, 30% output
  const inputTokens = estimatedTokens.value * 0.7
  const outputTokens = estimatedTokens.value * 0.3
  const cost = (inputTokens * pricing.input + outputTokens * pricing.output) / 1_000_000
  return cost
})

const tooltipText = computed(() => {
  const est = estimatedTokens.value.toLocaleString()
  const max = maxTokens.value.toLocaleString()
  let text = `~${est} / ${max} tokens used`
  if (estimatedCost.value !== null) {
    text += ` · ~$${estimatedCost.value.toFixed(4)}`
  }
  return text
})
</script>
