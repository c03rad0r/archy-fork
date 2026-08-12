<template>
  <button
    class="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl font-medium text-sm transition-all active:scale-95"
    :class="isDark
      ? 'bg-gradient-to-r from-[#F7931A] to-[#E8850F] text-white hover:brightness-110'
      : 'bg-gradient-to-r from-[#F7931A] to-[#E8850F] text-white hover:brightness-110'"
    @click="handleClick"
  >
    <svg class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
      <path d="M23.638 14.904c-1.602 6.43-8.113 10.34-14.542 8.736C2.67 22.05-1.244 15.525.362 9.105 1.962 2.67 8.475-1.243 14.9.358c6.43 1.605 10.342 8.115 8.738 14.546z" />
      <path fill="rgba(0,0,0,0.3)" d="M14.434 10.17c-.2-1.348-1.304-1.796-2.79-2.026l.57-2.285-1.39-.347-.555 2.225c-.366-.091-.74-.177-1.113-.263l.558-2.24-1.39-.347-.57 2.285c-.303-.069-.6-.137-.887-.21l.002-.007-1.918-.48-.37 1.486s1.032.237 1.01.251c.564.141.666.514.649.81l-.65 2.607c.039.01.089.024.144.046l-.147-.037-.91 3.654c-.069.171-.244.428-.64.33.015.021-1.01-.252-1.01-.252l-.69 1.593 1.81.452c.337.084.667.173.993.256l-.576 2.312 1.39.347.57-2.287c.38.103.748.198 1.108.288l-.568 2.273 1.39.347.576-2.308c2.372.449 4.156.268 4.907-1.877.605-1.728-.03-2.726-1.279-3.376.91-.21 1.596-.808 1.779-2.044zm-3.183 4.464c-.43 1.728-3.339.793-4.283.559l.764-3.064c.944.236 3.968.703 3.52 2.505zm.43-4.489c-.393 1.572-2.813.773-3.6.577l.693-2.778c.787.196 3.316.562 2.907 2.2z" />
    </svg>
    <span v-if="amount">{{ formattedAmount }}</span>
    <span v-else>Pay with Lightning</span>
  </button>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { createLightningUri, formatSats } from '@/utils/lightning'

const { isDark } = useTheme()

const props = defineProps<{
  invoice: string
  amount?: number
}>()

const emit = defineEmits<{
  'click': []
  'copy': [invoice: string]
}>()

const formattedAmount = computed(() => {
  if (!props.amount) return ''
  return formatSats(props.amount)
})

function handleClick() {
  const uri = createLightningUri(props.invoice)
  window.open(uri, '_blank')
  emit('click')
}
</script>
