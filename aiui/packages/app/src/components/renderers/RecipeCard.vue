<template>
  <div class="recipe-card rounded-xl bg-white/5 border border-white/10 overflow-hidden">
    <!-- Header -->
    <div class="px-4 py-3 border-b border-white/5">
      <h3 class="text-sm font-semibold text-white/90">{{ recipe.title }}</h3>
      <div class="flex gap-3 mt-1.5">
        <span v-if="recipe.time" class="text-xs text-white/40 flex items-center gap-1">
          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
          {{ recipe.time }}
        </span>
        <span v-if="recipe.servings" class="text-xs text-white/40 flex items-center gap-1">
          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
          {{ scaledServings }} servings
        </span>
        <span v-if="recipe.calories" class="text-xs text-white/40">{{ recipe.calories }} cal</span>
      </div>
    </div>

    <!-- Scale slider -->
    <div class="px-4 py-2 border-b border-white/5 flex items-center gap-3">
      <label class="text-xs text-white/30">Scale</label>
      <input
        v-model.number="scaleFactor"
        type="range"
        min="0.5"
        max="4"
        step="0.5"
        class="flex-1 h-1 accent-[#F7931A] bg-white/10 rounded-full appearance-none cursor-pointer"
      />
      <span class="text-xs text-white/50 tabular-nums w-8 text-right">{{ scaleFactor }}×</span>
    </div>

    <!-- Ingredients -->
    <div class="px-4 py-3 border-b border-white/5">
      <h4 class="text-xs text-white/40 uppercase tracking-wider mb-2">Ingredients</h4>
      <ul class="space-y-1">
        <li
          v-for="(ing, i) in scaledIngredients"
          :key="i"
          class="flex items-start gap-2 text-xs cursor-pointer select-none"
          :class="checkedIngredients.has(i) ? 'line-through text-white/30' : 'text-white/70'"
          @click="toggleIngredient(i)"
        >
          <span class="shrink-0 mt-0.5 w-4 h-4 rounded border flex items-center justify-center transition-colors"
            :class="checkedIngredients.has(i) ? 'border-accent/50 bg-accent/20' : 'border-white/20'">
            <svg v-if="checkedIngredients.has(i)" class="w-2.5 h-2.5 text-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" /></svg>
          </span>
          {{ ing }}
        </li>
      </ul>
    </div>

    <!-- Steps -->
    <div class="px-4 py-3">
      <h4 class="text-xs text-white/40 uppercase tracking-wider mb-2">Steps</h4>
      <ol class="space-y-2">
        <li
          v-for="(step, i) in recipe.steps"
          :key="i"
          class="flex gap-2 text-xs text-white/70"
        >
          <span class="shrink-0 w-5 h-5 rounded-full bg-accent/15 text-accent text-xs flex items-center justify-center font-medium">{{ i + 1 }}</span>
          <span class="leading-relaxed">{{ step }}</span>
        </li>
      </ol>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, reactive } from 'vue'

export interface Recipe {
  title: string
  servings?: string
  time?: string
  calories?: string
  ingredients: string[]
  steps: string[]
}

const props = defineProps<{ recipe: Recipe }>()

const scaleFactor = ref(1)
const checkedIngredients = reactive(new Set<number>())

const scaledServings = computed(() => {
  const base = parseInt(props.recipe.servings || '0', 10)
  if (!base) return props.recipe.servings
  return Math.round(base * scaleFactor.value)
})

// Scale numeric quantities in ingredients
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
