<template>
  <div class="flex-1 overflow-y-auto p-4">
    <!-- Poster grid: films, TV, books -->
    <div v-if="variant === 'poster'" class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-4">
      <div
        v-for="i in count"
        :key="i"
        class="aspect-[2/3] rounded-xl animate-pulse"
        :class="isDark ? 'bg-white/10' : 'bg-black/6'"
      />
    </div>

    <!-- Square grid: songs, podcasts, images -->
    <div v-else-if="variant === 'square'" class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-3">
      <div v-for="i in count" :key="i" class="space-y-2">
        <div
          class="aspect-square rounded-xl animate-pulse"
          :class="isDark ? 'bg-white/10' : 'bg-black/6'"
        />
        <div
          class="h-3 rounded animate-pulse w-3/4"
          :class="isDark ? 'bg-white/8' : 'bg-black/5'"
        />
        <div
          class="h-2.5 rounded animate-pulse w-1/2"
          :class="isDark ? 'bg-white/5' : 'bg-black/3'"
        />
      </div>
    </div>

    <!-- List: news, websites -->
    <div v-else-if="variant === 'list'" class="space-y-3">
      <div
        v-for="i in count"
        :key="i"
        class="flex gap-3 p-3 rounded-xl animate-pulse"
        :class="isDark ? 'bg-white/[0.04]' : 'bg-black/[0.03]'"
      >
        <div
          class="w-20 h-14 rounded-lg shrink-0"
          :class="isDark ? 'bg-white/10' : 'bg-black/6'"
        />
        <div class="flex-1 space-y-2 py-1">
          <div
            class="h-3 rounded w-4/5"
            :class="isDark ? 'bg-white/10' : 'bg-black/6'"
          />
          <div
            class="h-2.5 rounded w-3/5"
            :class="isDark ? 'bg-white/6' : 'bg-black/4'"
          />
        </div>
      </div>
    </div>

    <!-- Magazine: tile-style skeleton -->
    <div v-else-if="variant === 'magazine'" class="space-y-0">
      <!-- Hero skeleton -->
      <div
        class="h-44 animate-pulse mb-px"
        :class="isDark ? 'bg-white/[0.04]' : 'bg-black/[0.03]'"
      />
      <!-- Tile grid skeleton -->
      <div class="grid grid-cols-2 gap-px"
           :class="isDark ? 'bg-white/12' : 'bg-black/10'">
        <div
          v-for="i in count"
          :key="i"
          class="p-4 animate-pulse"
          :class="[
            isDark ? 'bg-[#0a0a0a]' : 'bg-[#faf9f6]',
            i <= 1 ? 'col-span-2' : ''
          ]"
        >
          <div
            class="h-2.5 rounded w-1/3 mb-2"
            :class="isDark ? 'bg-white/8' : 'bg-black/5'"
          />
          <div
            class="h-4 rounded w-4/5 mb-2"
            :class="isDark ? 'bg-white/10' : 'bg-black/6'"
          />
          <div
            class="h-2.5 rounded w-full"
            :class="isDark ? 'bg-white/6' : 'bg-black/4'"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useTheme } from '@/composables/useTheme'

withDefaults(
  defineProps<{
    variant?: 'poster' | 'square' | 'list' | 'magazine'
    count?: number
  }>(),
  { variant: 'poster', count: 8 }
)

const { isDark } = useTheme()
</script>
