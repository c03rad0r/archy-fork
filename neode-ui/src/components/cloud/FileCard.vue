<template>
  <div
    role="button"
    @keydown.enter.self.prevent="handleClick"
    @keydown.space.self.prevent="handleClick"
    class="cloud-file-item group"
    data-controller-container
    data-controller-primary
    tabindex="0"
    @click="handleClick"
  >
    <!-- Thumbnail / Icon -->
    <div class="cloud-file-item-thumb">
      <img
        v-if="isImage && thumbnailUrl && !imgFailed"
        :src="thumbnailUrl"
        :alt="item.name"
        class="w-full h-full object-cover rounded-[6px] transition-transform duration-300 group-hover:scale-105"
        loading="lazy"
        @error="imgFailed = true"
      />
      <div v-else class="w-full h-full rounded-[6px] flex items-center justify-center bg-white/8">
        <svg class="w-5 h-5" :class="iconColor" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            v-for="(d, i) in iconPaths"
            :key="i"
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            :d="d"
          />
        </svg>
      </div>
    </div>

    <!-- Content -->
    <div class="min-w-0 flex-1 py-0.5">
      <p class="text-sm font-semibold truncate text-white/90">{{ item.name }}</p>
      <p class="text-xs mt-0.5 text-white/40">
        <span v-if="!item.isDir">{{ formatSize(item.size) }}</span>
        <span v-if="!item.isDir"> · </span>
        <span>{{ formatDate(item.modified) }}</span>
      </p>
      <!-- Type badge -->
      <div class="flex items-center gap-1.5 mt-1.5">
        <span class="cloud-file-badge" :class="badgeClass">
          {{ badgeLabel }}
        </span>
        <span v-if="item.extension && !item.isDir" class="cloud-file-badge bg-white/8 text-white/50">
          .{{ item.extension }}
        </span>
      </div>
    </div>

      <FileActionsMenu class="cloud-card-menu" :name="item.name" :download-href="item.isDir ? undefined : downloadHref"
        can-share :can-delete="!item.isDir"
        @share="emit('share', item.path, item.name, item.isDir)" @delete="emit('delete', item.path)" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import FileActionsMenu from './FileActionsMenu.vue'
import type { FileBrowserItem } from '@/api/filebrowser-client'
import { useCloudStore } from '@/stores/cloud'
import { useFileType, formatSize, formatDate } from '@/composables/useFileType'

const props = defineProps<{
  item: FileBrowserItem
}>()

const emit = defineEmits<{
  navigate: [path: string]
  delete: [path: string]
  share: [path: string, name: string, isDir: boolean]
  preview: [path: string]
  play: [path: string, name: string]
}>()

const cloudStore = useCloudStore()
const imgFailed = ref(false)

const ext = computed(() => props.item.extension)
const isDir = computed(() => props.item.isDir)
const { isImage, isVideo, isAudio, iconPaths, iconColor, badgeLabel, badgeClass } = useFileType(ext, isDir)

const thumbnailUrl = computed(() => {
  if (!isImage.value || imgFailed.value) return null
  return cloudStore.downloadUrl(props.item.path)
})

const downloadHref = computed(() => cloudStore.downloadUrl(props.item.path))

function handleClick() {
  if (props.item.isDir) {
    emit('navigate', props.item.path)
  } else if (isAudio.value) {
    // Music goes to the global bottom-bar player, not the lightbox.
    emit('play', props.item.path, props.item.name)
  } else if (isImage.value || isVideo.value) {
    emit('preview', props.item.path)
  } else {
    // Non-media files open by downloading — a click should always act on the file.
    window.location.href = downloadHref.value
  }
}
</script>

<style scoped>
.cloud-card-menu { margin-left: auto; flex: none; }
</style>
