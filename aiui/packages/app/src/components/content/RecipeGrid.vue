<template>
  <div class="h-full flex flex-col">
    <div class="p-4 space-y-3" :style="isDark
      ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
      : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'"
    >
      <div class="flex items-center justify-between gap-2">
        <h3 class="text-sm font-bold" :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ title }}
        </h3>
        <div class="flex items-center gap-2 shrink-0">
          <span class="text-xs font-mono" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            {{ filteredRecipes.length }} recipes
          </span>
          <slot name="header-actions" />
        </div>
      </div>

      <input
        v-if="recipes.length > 3"
        v-model="search"
        type="text"
        placeholder="Search recipes..."
        class="w-full px-3 py-2 rounded-lg text-base outline-none transition-colors"
        :class="isDark
          ? 'bg-white/5 text-white/80 placeholder:text-white/25 focus:bg-white/10'
          : 'bg-black/3 text-gray-800 placeholder:text-gray-400 focus:bg-black/5'"
      />
    </div>

    <div class="flex-1 overflow-y-auto custom-scrollbar px-4 pt-4 pb-16">
      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <button
          v-for="(recipe, i) in filteredRecipes"
          :key="i"
          class="group flex flex-col items-stretch text-left w-full rounded-2xl overflow-hidden transition-all duration-200 hover:brightness-105"
          :class="isDark
            ? 'bg-white/[0.04] border border-white/8 hover:bg-white/[0.07]'
            : 'bg-black/[0.02] border border-black/5 hover:bg-black/[0.05]'"
          :aria-label="recipe.title"
          @click="$emit('selectRecipe', recipe)"
        >
          <!-- Cover image area -->
          <div class="aspect-[3/1] relative w-full overflow-hidden">
            <img
              :src="generateRecipeFallback(recipe.title, recipe.time)"
              :alt="recipe.title"
              class="w-full h-full object-cover"
            />
          </div>

          <!-- Card body -->
          <div class="p-3">
            <p class="text-sm font-semibold leading-tight line-clamp-2"
               :class="isDark ? 'text-white/90' : 'text-gray-900'">
              {{ recipe.title }}
            </p>

            <!-- Meta row -->
            <div class="flex items-center gap-3 mt-2 flex-wrap">
              <span v-if="recipe.time" class="flex items-center gap-1 text-xs"
                    :class="isDark ? 'text-white/40' : 'text-gray-500'">
                <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                {{ recipe.time }}
              </span>
              <span v-if="recipe.servings" class="flex items-center gap-1 text-xs"
                    :class="isDark ? 'text-white/40' : 'text-gray-500'">
                <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
                {{ recipe.servings }}
              </span>
              <span v-if="recipe.calories" class="text-xs"
                    :class="isDark ? 'text-white/40' : 'text-gray-500'">
                {{ recipe.calories }} cal
              </span>
            </div>

            <!-- Ingredient preview -->
            <p v-if="recipe.ingredients.length > 0" class="text-xs mt-2 line-clamp-1"
               :class="isDark ? 'text-white/25' : 'text-gray-400'">
              {{ recipe.ingredients.slice(0, 4).join(' \u00B7 ') }}
            </p>
          </div>
        </button>
      </div>

      <div v-if="filteredRecipes.length === 0" class="flex items-center justify-center py-12">
        <p class="text-sm" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          No recipes match your search
        </p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import type { RecipeData } from '@/composables/contentExtraction'
import { useTheme } from '@/composables/useTheme'
import { generateRecipeFallback } from '@/composables/useImageFallback'

const props = withDefaults(defineProps<{
  recipes: RecipeData[]
  title?: string
}>(), {
  title: 'Recipes',
})

defineEmits<{ selectRecipe: [recipe: RecipeData] }>()

const { isDark } = useTheme()
const search = ref('')

const filteredRecipes = computed(() => {
  if (!search.value.trim()) return props.recipes
  const q = search.value.toLowerCase()
  return props.recipes.filter(r =>
    r.title.toLowerCase().includes(q) ||
    r.ingredients.some(ing => ing.toLowerCase().includes(q))
  )
})
</script>
