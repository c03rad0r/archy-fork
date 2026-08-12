<template>
  <!-- Floating Filter Button (teleported to escape CSS transform containing block) -->
  <Teleport to="body">
    <button
      @click="showModal = true"
      class="md:hidden fixed right-4 z-[2400] w-14 h-14 rounded-full glass-button flex items-center justify-center shadow-2xl mobile-filter-btn"
    >
      <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
      </svg>
    </button>
  </Teleport>

  <!-- Filter Modal (Mobile only) -->
  <Transition name="modal">
    <div
      v-if="showModal"
      class="fixed inset-0 z-[3000] flex items-end justify-center md:hidden bg-black/60 backdrop-blur-md"
      @click.self="close()"
    >
      <div ref="modalRef" class="glass-card p-6 w-full rounded-t-3xl max-h-[80vh] overflow-y-auto mobile-filter-sheet">
        <!-- Header -->
        <div class="flex items-center justify-between mb-6">
          <h2 class="text-2xl font-bold text-white">{{ t('marketplace.filterByCategory') }}</h2>
          <button
            @click="close()"
            class="text-white/60 hover:text-white transition-colors"
          >
            <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- Category Grid -->
        <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <button
            v-for="category in categories"
            :key="category.id"
            @click="$emit('select', category.id); close()"
            :class="[
              'p-4 rounded-xl font-medium transition-all text-left',
              selectedCategory === category.id
                ? 'bg-white/20 text-white border-2 border-white/40'
                : 'glass-card text-white/80 hover:bg-white/10'
            ]"
          >
            <div class="flex items-center gap-3">
              <!-- Category Icon -->
              <div class="flex-shrink-0 w-10 h-10 rounded-lg bg-white/10 flex items-center justify-center">
                <svg v-if="category.id === 'all'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                </svg>
                <svg v-else-if="category.id === 'community'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
                </svg>
                <svg v-else-if="category.id === 'nostr'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
                <svg v-else-if="category.id === 'commerce'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 3h2l.4 2M7 13h10l4-8H5.4M7 13L5.4 5M7 13l-2.293 2.293c-.63.63-.184 1.707.707 1.707H17m0 0a2 2 0 100 4 2 2 0 000-4zm-8 2a2 2 0 11-4 0 2 2 0 014 0z" />
                </svg>
                <svg v-else-if="category.id === 'money'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                </svg>
                <svg v-else-if="category.id === 'data'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />
                </svg>
                <svg v-else-if="category.id === 'home'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
                </svg>
                <svg v-else-if="category.id === 'car'" class="w-6 h-6" viewBox="0 0 122.88 122.88" fill="currentColor">
                  <path d="M61.44,0c33.93,0,61.44,27.51,61.44,61.44c0,33.93-27.51,61.44-61.44,61.44S0,95.37,0,61.44 C0,27.51,27.51,0,61.44,0L61.44,0z M61.17,61.6c1.76,0,3.18,1.42,3.18,3.18c0,1.76-1.42,3.18-3.18,3.18 c-1.76,0-3.18-1.42-3.18-3.18C57.99,63.03,59.42,61.6,61.17,61.6L61.17,61.6z M61.2,53.28c6.34,0,11.47,5.14,11.47,11.47 c0,6.34-5.14,11.47-11.47,11.47c-6.33,0-11.47-5.14-11.47-11.47C49.73,58.41,54.87,53.28,61.2,53.28L61.2,53.28z M14.78,44.57 c4.45-12.31,13.52-22.7,24.9-28.01c15.63-7.29,34.61-7.75,50.69,4.15c9.48,7.01,12.94,12.76,17.67,22.95 c3.58,9.03,0.64,11.97-10.87,6.9c-23.79-11.77-47.84-11.24-72.12,0C16.09,56.41,11.06,51.53,14.78,44.57L14.78,44.57z M75.9,109.05 c16.62-5.23,26.32-15.81,32.27-29.3c3.87-10.43-8.26-13.97-12.52-7.1c-2.55,5.06-5.59,9.4-9.55,12.77 c-6.2,5.27-15.18,6.23-16.58,16.16C68.79,106.74,69.97,111.38,75.9,109.05L75.9,109.05z M47.26,109.05 c-16.62-5.23-26.32-15.81-32.27-29.3c-3.87-10.43,8.26-13.97,12.52-7.1c2.55,5.06,5.59,9.4,9.55,12.77 c6.2,5.27,15.18,6.23,16.58,16.16C54.37,106.74,53.19,111.38,47.26,109.05L47.26,109.05z"/>
                </svg>
                <svg v-else-if="category.id === 'networking'" class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9m-9 9a9 9 0 019-9" />
                </svg>
                <svg v-else class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                </svg>
              </div>
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
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useModalKeyboard } from '@/composables/useModalKeyboard'

const { t } = useI18n()

defineProps<{
  categories: { id: string; name: string }[]
  selectedCategory: string
}>()

defineEmits<{
  select: [categoryId: string]
}>()

const showModal = ref(false)
const modalRef = ref<HTMLElement | null>(null)
const restoreFocusRef = ref<HTMLElement | null>(null)

function close() {
  restoreFocusRef.value?.focus?.()
  showModal.value = false
}

useModalKeyboard(modalRef, showModal, close, { restoreFocusRef })
</script>
