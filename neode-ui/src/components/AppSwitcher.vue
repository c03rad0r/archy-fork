<template>
  <div class="relative" ref="containerRef">
    <!-- Mobile/tablet: Online button launches CLI directly (no CLI label, no dropdown) -->
    <button
      type="button"
      class="lg:hidden flex items-center gap-2 px-3 py-2 rounded-lg glass-card text-white/90 hover:bg-white/10 hover:text-white transition-colors min-w-0 border border-white/10"
      @click="selectCLI"
    >
      <img
        src="/assets/img/logo-archipelago.svg"
        alt="Archipelago"
        class="w-5 h-5 shrink-0 object-contain opacity-90"
      />
      <div class="flex items-center gap-1.5 shrink-0">
        <div class="relative">
          <div class="w-2 h-2 rounded-full bg-green-400"></div>
          <div class="absolute inset-0 w-2 h-2 rounded-full bg-green-400 animate-ping opacity-50"></div>
        </div>
        <span class="text-xs text-white/80">Online</span>
      </div>
    </button>

    <!-- Desktop: Full switcher with dropdown (lg and up) -->
    <button
      type="button"
      class="hidden lg:flex items-center gap-2 px-3 py-2 rounded-lg glass-card text-white/90 hover:bg-white/10 hover:text-white transition-colors min-w-0 border border-white/10"
      @click="showDropdown = !showDropdown"
    >
      <img
        src="/assets/img/logo-archipelago.svg"
        alt="Archipelago"
        class="w-5 h-5 shrink-0 object-contain opacity-90"
      />
      <span class="text-sm font-medium truncate max-w-[100px] sm:max-w-[120px]">Archipelago CLI</span>
      <div class="flex items-center gap-1.5 shrink-0 pl-1 border-l border-white/20">
        <div class="relative">
          <div class="w-2 h-2 rounded-full bg-green-400"></div>
          <div class="absolute inset-0 w-2 h-2 rounded-full bg-green-400 animate-ping opacity-50"></div>
        </div>
        <span class="text-xs text-white/80">Online</span>
      </div>
      <svg class="w-4 h-4 text-white/50 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <Transition name="dropdown">
      <div
        v-if="showDropdown"
        class="absolute right-0 top-full mt-1 py-1 min-w-[160px] rounded-lg glass-card shadow-xl z-50"
        @click.stop
      >
        <button
          type="button"
          class="w-full flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors"
          :class="inCLI ? 'bg-white/10 text-white' : 'text-white/80 hover:bg-white/10 hover:text-white'"
          @click="selectCLI"
        >
          <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
          Archipelago CLI
        </button>
        <button
          type="button"
          class="w-full flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors"
          :class="!inCLI ? 'bg-white/10 text-white' : 'text-white/80 hover:bg-white/10 hover:text-white'"
          @click="selectWebUI"
        >
          <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
          </svg>
          Web UI
        </button>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onBeforeUnmount } from 'vue'
import { useCLIStore } from '@/stores/cli'

const cliStore = useCLIStore()
const containerRef = ref<HTMLElement | null>(null)
const showDropdown = ref(false)

const inCLI = computed(() => cliStore.isOpen)

function selectCLI() {
  showDropdown.value = false
  cliStore.open()
}

function selectWebUI() {
  showDropdown.value = false
  cliStore.close()
}

function handleClickOutside(e: MouseEvent) {
  if (containerRef.value && !containerRef.value.contains(e.target as Node)) {
    showDropdown.value = false
  }
}

onMounted(() => {
  document.addEventListener('pointerdown', handleClickOutside)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', handleClickOutside)
})
</script>

<style scoped>
.dropdown-enter-active,
.dropdown-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}
.dropdown-enter-from,
.dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
