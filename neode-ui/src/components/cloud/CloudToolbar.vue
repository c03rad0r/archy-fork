<template>
  <div class="cloud-toolbar">
    <!-- Breadcrumbs -->
    <nav class="cloud-breadcrumbs">
      <button
        v-for="(crumb, i) in breadcrumbs"
        :key="crumb.path"
        class="cloud-breadcrumb-item"
        :class="{ 'cloud-breadcrumb-active': i === breadcrumbs.length - 1 }"
        @click="i < breadcrumbs.length - 1 && $emit('navigate', crumb.path)"
      >
        <svg v-if="i === 0" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6" />
        </svg>
        <span v-else>{{ crumb.name }}</span>
        <svg v-if="i < breadcrumbs.length - 1" class="w-3 h-3 text-white/30 mx-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </button>
    </nav>

    <!-- Actions -->
    <div class="flex items-center gap-2">
      <!-- View toggle -->
      <div class="cloud-view-toggle">
        <button
          class="cloud-view-toggle-btn"
          :class="{ 'cloud-view-toggle-active': viewMode === 'grid' }"
          title="Grid view"
          @click="$emit('update:viewMode', 'grid')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm10 0a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
          </svg>
        </button>
        <button
          class="cloud-view-toggle-btn"
          :class="{ 'cloud-view-toggle-active': viewMode === 'list' }"
          title="List view"
          @click="$emit('update:viewMode', 'list')"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 10h16M4 14h16M4 18h16" />
          </svg>
        </button>
      </div>

      <button class="glass-button cloud-toolbar-btn" title="Upload file" @click="triggerUpload">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
        </svg>
        <span class="hidden md:inline">Upload</span>
      </button>
      <button class="glass-button cloud-toolbar-btn" title="Refresh" @click="$emit('refresh')">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
    </div>

    <input
      ref="fileInput"
      type="file"
      class="hidden"
      multiple
      @change="handleFileSelect"
    />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

defineProps<{
  breadcrumbs: { name: string; path: string }[]
  viewMode: 'list' | 'grid'
}>()

const emit = defineEmits<{
  navigate: [path: string]
  refresh: []
  upload: [files: File[]]
  'update:viewMode': [mode: 'list' | 'grid']
}>()

const fileInput = ref<HTMLInputElement | null>(null)

function triggerUpload() {
  fileInput.value?.click()
}

function handleFileSelect(e: Event) {
  const input = e.target as HTMLInputElement
  if (input.files && input.files.length > 0) {
    emit('upload', Array.from(input.files))
    input.value = ''
  }
}
</script>
