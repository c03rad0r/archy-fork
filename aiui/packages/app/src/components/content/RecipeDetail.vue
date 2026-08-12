<template>
  <div class="h-full flex flex-col">
    <!-- Header -->
    <div class="shrink-0 px-4 py-3 flex items-center gap-2"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="touch-target rounded-lg transition-colors shrink-0"
        :class="isDark
          ? 'text-white/70 hover:bg-white/10'
          : 'text-gray-500 hover:bg-black/5'"
        aria-label="Back to recipes"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <h3 class="text-sm font-bold truncate" :class="isDark ? 'text-white/90' : 'text-gray-900'">
        {{ recipe.title }}
      </h3>
    </div>

    <!-- Cover banner -->
    <div class="shrink-0 h-28 relative overflow-hidden">
      <img
        :src="generateRecipeFallback(recipe.title, recipe.time)"
        :alt="recipe.title"
        class="w-full h-full object-cover"
      />
    </div>

    <!-- Recipe content (reuses RecipeCard logic inline) -->
    <div class="flex-1 overflow-y-auto custom-scrollbar">
      <!-- Meta -->
      <div class="px-4 py-3 flex gap-3 flex-wrap"
           :style="isDark
             ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.05)'
             : 'border-bottom: 1px solid rgba(0, 0, 0, 0.04)'">
        <span v-if="recipe.time" class="text-xs flex items-center gap-1"
              :class="isDark ? 'text-white/50' : 'text-gray-500'">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          {{ recipe.time }}
        </span>
        <span v-if="recipe.servings" class="text-xs flex items-center gap-1"
              :class="isDark ? 'text-white/50' : 'text-gray-500'">
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          {{ scaledServings }} servings
        </span>
        <span v-if="recipe.calories" class="text-xs"
              :class="isDark ? 'text-white/50' : 'text-gray-500'">
          {{ recipe.calories }} cal
        </span>
      </div>

      <!-- Scale slider -->
      <div class="px-4 py-3 flex items-center gap-3"
           :style="isDark
             ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.05)'
             : 'border-bottom: 1px solid rgba(0, 0, 0, 0.04)'">
        <label class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">Scale</label>
        <input
          v-model.number="scaleFactor"
          type="range"
          min="0.5"
          max="4"
          step="0.5"
          class="flex-1 h-1 accent-accent rounded-full appearance-none cursor-pointer"
          :class="isDark ? 'bg-white/10' : 'bg-black/10'"
        />
        <span class="text-xs tabular-nums w-8 text-right"
              :class="isDark ? 'text-white/50' : 'text-gray-500'">{{ scaleFactor }}x</span>
      </div>

      <!-- Ingredients -->
      <div class="px-4 py-4"
           :style="isDark
             ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.05)'
             : 'border-bottom: 1px solid rgba(0, 0, 0, 0.04)'">
        <h4 class="text-xs uppercase tracking-wider mb-3 font-medium"
            :class="isDark ? 'text-white/40' : 'text-gray-400'">
          Ingredients
        </h4>
        <ul class="space-y-2">
          <li
            v-for="(ing, i) in scaledIngredients"
            :key="i"
            class="flex items-start gap-2.5 text-sm cursor-pointer select-none transition-colors"
            :class="checkedIngredients.has(i)
              ? isDark ? 'line-through text-white/25' : 'line-through text-gray-300'
              : isDark ? 'text-white/70' : 'text-gray-700'"
            @click="toggleIngredient(i)"
          >
            <span class="shrink-0 mt-0.5 w-5 h-5 rounded border flex items-center justify-center transition-colors"
              :class="checkedIngredients.has(i)
                ? 'border-accent/50 bg-accent/20'
                : isDark ? 'border-white/20' : 'border-gray-300'">
              <svg v-if="checkedIngredients.has(i)" class="w-3 h-3 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
              </svg>
            </span>
            {{ ing }}
          </li>
        </ul>
      </div>

      <!-- Steps -->
      <div class="px-4 py-4 pb-16">
        <h4 class="text-xs uppercase tracking-wider mb-3 font-medium"
            :class="isDark ? 'text-white/40' : 'text-gray-400'">
          Steps
        </h4>
        <ol class="space-y-3">
          <li
            v-for="(step, i) in recipe.steps"
            :key="i"
            class="flex gap-3 text-sm"
            :class="isDark ? 'text-white/70' : 'text-gray-700'"
          >
            <span class="shrink-0 w-6 h-6 rounded-full bg-accent/15 text-accent text-xs flex items-center justify-center font-semibold">
              {{ i + 1 }}
            </span>
            <span class="leading-relaxed pt-0.5">{{ step }}</span>
          </li>
        </ol>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive } from 'vue'
import type { RecipeData } from '@/composables/contentExtraction'
import { useTheme } from '@/composables/useTheme'
import { generateRecipeFallback } from '@/composables/useImageFallback'

const props = defineProps<{ recipe: RecipeData }>()
defineEmits<{ back: [] }>()

const { isDark } = useTheme()
const scaleFactor = ref(1)
const checkedIngredients = reactive(new Set<number>())

const scaledServings = computed(() => {
  const base = parseInt(props.recipe.servings || '0', 10)
  if (!base) return props.recipe.servings
  return Math.round(base * scaleFactor.value)
})

const scaledIngredients = computed(() => {
  return props.recipe.ingredients.map((ing) =>
    ing.replace(/(\d+\.?\d*)/g, (match) => {
      const num = parseFloat(match)
      const scaled = num * scaleFactor.value
      return scaled % 1 === 0 ? String(scaled) : scaled.toFixed(1)
    })
  )
})

function toggleIngredient(idx: number) {
  if (checkedIngredients.has(idx)) {
    checkedIngredients.delete(idx)
  } else {
    checkedIngredients.add(idx)
  }
}
</script>
