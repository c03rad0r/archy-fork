<template>
  <div class="flex-1 min-h-0 overflow-y-auto">
    <!-- Loading skeleton -->
    <div v-if="loading" :class="viewMode === 'grid' ? 'cloud-card-grid' : 'cloud-file-list'">
      <div
        v-for="i in 6"
        :key="i"
        :class="viewMode === 'grid' ? 'cloud-grid-card-skeleton' : 'cloud-file-item cloud-file-item-skeleton'"
      >
        <template v-if="viewMode === 'grid'">
          <div class="aspect-square rounded-[10px] bg-white/8 animate-pulse"></div>
        </template>
        <template v-else>
          <div class="cloud-file-item-thumb">
            <div class="w-full h-full rounded-[6px] bg-white/8 animate-pulse"></div>
          </div>
          <div class="min-w-0 flex-1 py-0.5">
            <div class="h-4 w-32 rounded bg-white/8 animate-pulse mb-1.5"></div>
            <div class="h-3 w-20 rounded bg-white/5 animate-pulse"></div>
          </div>
        </template>
      </div>
    </div>

    <!-- Empty state -->
    <div v-else-if="items.length === 0" class="flex flex-col items-center justify-center py-16 text-center">
      <svg class="w-16 h-16 text-white/10 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
      </svg>
      <p class="text-white/50 text-sm">This folder is empty</p>
      <p class="text-white/30 text-xs mt-1">Upload files to get started</p>
    </div>

    <!-- Grid view -->
    <div v-else-if="viewMode === 'grid'" class="cloud-card-grid">
      <FileCardGrid
        v-for="item in items"
        :key="item.path"
        :item="item"
        @navigate="$emit('navigate', $event)"
        @delete="$emit('delete', $event)"
        @play="(path, name) => $emit('play', path, name)"
        @share="(path, name, isDir) => $emit('share', path, name, isDir)"
        @preview="$emit('preview', $event)"
      />
    </div>

    <!-- List view -->
    <div v-else class="cloud-file-list">
      <FileCard
        v-for="item in items"
        :key="item.path"
        :item="item"
        @navigate="$emit('navigate', $event)"
        @delete="$emit('delete', $event)"
        @play="(path, name) => $emit('play', path, name)"
        @share="(path, name, isDir) => $emit('share', path, name, isDir)"
        @preview="$emit('preview', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import type { FileBrowserItem } from '@/api/filebrowser-client'
import FileCard from './FileCard.vue'
import FileCardGrid from './FileCardGrid.vue'

withDefaults(defineProps<{
  items: FileBrowserItem[]
  loading: boolean
  viewMode?: 'list' | 'grid'
}>(), {
  viewMode: 'grid',
})

defineEmits<{
  navigate: [path: string]
  delete: [path: string]
  play: [path: string, name: string]
  share: [path: string, name: string, isDir: boolean]
  preview: [path: string]
}>()
</script>
