<template>
  <button
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

    <!-- Actions -->
    <div class="cloud-file-item-actions" @click.stop>
      <button
        class="cloud-file-action-btn cloud-file-action-share"
        title="Share with peers"
        @click.stop="$emit('share', item.path, item.name, item.isDir)"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z" />
        </svg>
      </button>
      <a
        v-if="!item.isDir"
        :href="downloadHref"
        download
        class="cloud-file-action-btn"
        title="Download"
        @click.stop
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
        </svg>
      </a>
      <button
        v-if="!item.isDir"
        class="cloud-file-action-btn cloud-file-action-delete"
        title="Delete"
        @click.stop="$emit('delete', item.path)"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
        </svg>
      </button>
      <svg v-if="item.isDir" class="w-4 h-4 text-white/30" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
      </svg>
    </div>
  </button>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
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
