<template>
  <button
    class="w-full text-left px-3 py-2 rounded-lg text-xs transition-all flex items-center gap-2"
    :class="destructive
      ? 'text-red-400/80 hover:text-red-400 hover:bg-white/10'
      : 'text-white/60 hover:text-white hover:bg-white/10'"
    role="menuitem"
    @click="$emit('click')"
  >
    <slot />
  </button>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    destructive?: boolean
  }>(),
  { destructive: false }
)

defineEmits<{
  click: []
}>()
</script>
