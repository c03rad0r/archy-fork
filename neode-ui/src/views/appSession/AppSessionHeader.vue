<template>
  <div class="sticky top-0 z-10 hidden md:flex items-center gap-3 border-b border-white/10 px-4 py-3 bg-black/60 backdrop-blur-md md:bg-transparent md:backdrop-blur-none">
    <!-- Back / Forward navigation -->
    <div class="flex items-center gap-0.5">
      <button class="app-session-btn" aria-label="Back" title="Go back" @click="$emit('goBack')">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>
      <button class="app-session-btn" aria-label="Forward" title="Go forward" @click="$emit('goForward')">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </div>

    <span class="flex-1 truncate text-sm font-medium text-white/90">{{ appTitle }}</span>

    <button class="app-session-btn" aria-label="Refresh" :disabled="isRefreshing" @click="$emit('refresh')">
      <svg class="w-5 h-5 transition-transform duration-300" :class="{ 'animate-spin': isRefreshing }" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
      </svg>
    </button>

    <!-- Display mode: one-click switch -->
    <div class="flex items-center gap-0.5">
      <button
        class="app-session-btn"
        :class="{ 'app-session-btn-active': displayMode === 'panel' }"
        aria-label="Right panel"
        title="Right panel"
        @click="$emit('setMode', 'panel')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 3v18m12-18H3a1 1 0 00-1 1v16a1 1 0 001 1h18a1 1 0 001-1V4a1 1 0 00-1-1z" />
        </svg>
      </button>
      <button
        class="app-session-btn"
        :class="{ 'app-session-btn-active': displayMode === 'overlay' }"
        aria-label="Over whole app"
        title="Over whole app"
        @click="$emit('setMode', 'overlay')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1H5a1 1 0 01-1-1V5z" />
        </svg>
      </button>
      <button
        class="app-session-btn"
        :class="{ 'app-session-btn-active': displayMode === 'fullscreen' }"
        aria-label="Open fullscreen"
        title="Open fullscreen"
        @click="$emit('setMode', 'fullscreen')"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 8V4m0 0h4M4 4l5 5m11-1V4m0 0h-4m4 0l-5 5M4 16v4m0 0h4m-4 0l5-5m11 5v-4m0 4h-4m4 0l-5-5" />
        </svg>
      </button>
    </div>

    <button class="app-session-btn" aria-label="Open in new tab" title="Open in new tab" @click="$emit('openNewTab')">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
      </svg>
    </button>

    <button class="app-session-btn" aria-label="Close" @click="$emit('close')">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>

    <kbd class="hidden sm:inline-flex px-2 py-1 text-xs text-white/50 bg-white/10 rounded">Esc</kbd>
  </div>
</template>

<script setup lang="ts">
import type { DisplayMode } from './appSessionConfig'

defineProps<{
  appTitle: string
  isRefreshing: boolean
  displayMode: DisplayMode
}>()

defineEmits<{
  goBack: []
  goForward: []
  refresh: []
  openNewTab: []
  close: []
  setMode: [mode: DisplayMode]
}>()
</script>
