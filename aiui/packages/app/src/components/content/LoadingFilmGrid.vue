<template>
  <div class="flex-1 overflow-y-auto p-4">
    <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
      <LoadingFilmCard
        v-for="i in count"
        :key="i"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import LoadingFilmCard from './LoadingFilmCard.vue'

withDefaults(
  defineProps<{
    count?: number
  }>(),
  { count: 12 }
)
</script>
