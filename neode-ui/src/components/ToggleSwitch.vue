<template>
  <button
    type="button"
    role="switch"
    :aria-checked="modelValue"
    :aria-label="ariaLabel"
    :disabled="disabled"
    tabindex="-1"
    data-controller-ignore
    class="w-10 h-6 rounded-full shrink-0 transition-colors relative"
    :class="[modelValue ? 'bg-orange-500' : 'bg-white/15', disabled ? 'opacity-40 cursor-not-allowed' : '']"
    @click="$emit('update:modelValue', !modelValue)"
  >
    <div
      class="absolute top-1 w-4 h-4 rounded-full bg-white shadow transition-transform"
      :class="modelValue ? 'translate-x-5' : 'translate-x-1'"
    />
  </button>
</template>

<script setup lang="ts">
defineProps<{
  modelValue: boolean
  disabled?: boolean
  ariaLabel?: string
}>()

defineEmits<{
  'update:modelValue': [value: boolean]
}>()
</script>
