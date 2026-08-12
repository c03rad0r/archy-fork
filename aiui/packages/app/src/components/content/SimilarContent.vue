<template>
  <div v-if="isLoading || suggestions.length > 0" class="mt-4 pt-4 border-t border-white/[0.08]">
    <p class="text-xs text-accent/60 uppercase tracking-wider font-bold mb-2">More Like This</p>

    <div v-if="isLoading" class="py-3">
      <p class="text-xs text-white/30">Finding similar content...</p>
    </div>

    <div v-else class="space-y-1.5">
      <div
        v-for="(item, i) in suggestions"
        :key="i"
        class="p-2.5 rounded-lg bg-white/[0.03] border border-white/5"
      >
        <p class="text-xs font-semibold text-white/80">{{ item.title }}</p>
        <p class="text-xs text-white/40 mt-0.5">{{ item.reason }}</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import { useSimilarContent } from '@/composables/useSimilarContent'
import type { FavoriteType } from '@/stores/favorites'

const props = defineProps<{
  itemId: string
  title: string
  type: FavoriteType
}>()

const { suggestions, isLoading, fetchSimilar } = useSimilarContent()

onMounted(() => {
  fetchSimilar(props.itemId, props.title, props.type)
})
</script>
