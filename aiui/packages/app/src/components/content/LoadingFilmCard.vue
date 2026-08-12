<template>
  <div
    class="aspect-[2/3] rounded-xl animate-pulse"
    :class="isDark ? 'bg-white/10' : 'bg-black/6'"
  />
</template>

<script setup lang="ts">
import { useTheme } from '@/composables/useTheme'

const { isDark } = useTheme()
</script>
