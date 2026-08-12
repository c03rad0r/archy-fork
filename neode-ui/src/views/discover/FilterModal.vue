<template>
  <div>
    <!-- Floating Filter Button (Mobile) -->
    <Teleport to="body">
      <button
        @click="showFilter = true"
        class="md:hidden fixed right-4 z-[2400] w-14 h-14 rounded-full glass-button flex items-center justify-center shadow-2xl mobile-filter-btn"
      >
        <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
        </svg>
      </button>
    </Teleport>

    <!-- Filter Modal (Mobile) -->
    <Transition name="modal">
      <div
        v-if="showFilter"
        class="fixed inset-0 z-[3000] flex items-end justify-center md:hidden bg-black/60 backdrop-blur-md"
        @click.self="closeFilter"
      >
        <div ref="filterModalRef" class="glass-card p-6 w-full rounded-t-3xl max-h-[80vh] overflow-y-auto mobile-filter-sheet">
          <div class="flex items-center justify-between mb-6">
            <h2 class="text-2xl font-bold text-white">Filter</h2>
            <button @click="closeFilter" class="text-white/60 hover:text-white transition-colors">
              <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <button
              v-for="category in categories"
              :key="category.id"
              @click="$emit('select-category', category.id); closeFilter()"
              :class="[
                'p-4 rounded-xl font-medium transition-all text-left',
                selectedCategory === category.id
                  ? 'bg-white/20 text-white border-2 border-white/40'
                  : 'glass-card text-white/80 hover:bg-orange-500/5 hover:border-orange-500/15'
              ]"
            >
              <div class="flex items-center gap-3">
                <div class="flex-1">
                  <p class="font-semibold">{{ category.name }}</p>
                  <p v-if="selectedCategory === category.id" class="text-xs text-white/60 mt-1">Currently viewing</p>
                </div>
                <svg v-if="selectedCategory === category.id" class="w-5 h-5 text-white flex-shrink-0" fill="currentColor" viewBox="0 0 20 20">
                  <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd" />
                </svg>
              </div>
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useModalKeyboard } from '@/composables/useModalKeyboard'
import type { CategoryDef } from './types'

defineProps<{
  categories: CategoryDef[]
  selectedCategory: string
}>()

defineEmits<{
  'select-category': [id: string]
}>()

const showFilter = ref(false)
const filterModalRef = ref<HTMLElement | null>(null)
const filterRestoreFocusRef = ref<HTMLElement | null>(null)

function closeFilter() {
  filterRestoreFocusRef.value?.focus?.()
  showFilter.value = false
}

useModalKeyboard(filterModalRef, showFilter, closeFilter, { restoreFocusRef: filterRestoreFocusRef })
</script>
