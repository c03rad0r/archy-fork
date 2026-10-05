<template>
  <div
    role="button"
    @keydown.enter.self.prevent="handleClick"
    @keydown.space.self.prevent="handleClick"
    class="cloud-grid-card group"
    data-controller-container
    tabindex="0"
    @click="handleClick"
  >
    <!-- Cover / Thumbnail area -->
    <div class="cloud-grid-card-cover">
      <!-- Image thumbnail -->
      <img
        v-if="isImage && thumbnailUrl && !imgFailed"
        :src="thumbnailUrl"
        :alt="item.name"
        class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
        loading="lazy"
        @error="imgFailed = true"
      />
      <!-- Video thumbnail (try to show, fallback to icon) -->
      <img
        v-else-if="isVideo && thumbnailUrl && !imgFailed"
        :src="thumbnailUrl"
        :alt="item.name"
        class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-110"
        loading="lazy"
        @error="imgFailed = true"
      />
      <!-- Icon fallback -->
      <div v-else class="w-full h-full flex items-center justify-center" :class="coverBg">
        <svg class="w-10 h-10" :class="iconColor" fill="none" stroke="currentColor" viewBox="0 0 24 24">
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

      <!-- Gradient overlay -->
      <div class="cloud-grid-card-gradient"></div>

      <!-- Play button overlay for audio/video -->
      <div
        v-if="isAudio || isVideo"
        class="cloud-grid-card-play"
        :class="{ 'cloud-grid-card-play-active': isCurrentlyPlaying }"
        @click.stop="isVideo ? emit('preview', item.path) : emit('play', item.path, item.name)"
      >
        <span class="cloud-grid-card-play-btn">
          <svg v-if="!isCurrentlyPlaying" class="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
            <path d="M8 5v14l11-7L8 5z" />
          </svg>
          <svg v-else class="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
            <path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z" />
          </svg>
        </span>
      </div>

      <!-- Info overlay at bottom -->
      <div class="cloud-grid-card-info">
        <p class="text-xs font-semibold text-white/90 leading-tight truncate">
          {{ item.name }}
        </p>
        <div class="flex items-center gap-1 mt-0.5">
          <span v-if="!item.isDir" class="text-xs text-white/40">{{ formatSize(item.size) }}</span>
          <span v-if="!item.isDir" class="text-xs text-white/40">·</span>
          <span class="text-xs text-white/40">{{ formatDate(item.modified) }}</span>
        </div>
      </div>

      <!-- Badge at top-right -->
      <div class="cloud-grid-card-badges">
        <span class="cloud-grid-card-badge" :class="badgeClass">
          {{ badgeLabel }}
        </span>
      </div>

      <FileActionsMenu class="cloud-card-menu" :name="item.name" :download-href="item.isDir ? undefined : downloadHref"
        can-share :can-delete="!item.isDir"
        @share="emit('share', item.path, item.name, item.isDir)" @delete="emit('delete', item.path)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import FileActionsMenu from './FileActionsMenu.vue'
import type { FileBrowserItem } from '@/api/filebrowser-client'
import { useCloudStore } from '@/stores/cloud'
import { useFileType, formatSize, formatDate } from '@/composables/useFileType'
import { useAudioPlayer } from '@/composables/useAudioPlayer'

const props = defineProps<{
  item: FileBrowserItem
}>()

const emit = defineEmits<{
  navigate: [path: string]
  delete: [path: string]
  play: [path: string, name: string]
  share: [path: string, name: string, isDir: boolean]
  preview: [path: string]
}>()

const cloudStore = useCloudStore()
const imgFailed = ref(false)

const ext = computed(() => props.item.extension)
const isDir = computed(() => props.item.isDir)
const { category, isImage, isAudio, isVideo, iconPaths, iconColor, badgeLabel, badgeClass } = useFileType(ext, isDir)

const thumbnailUrl = computed(() => {
  if (imgFailed.value) return null
  if (isImage.value || isVideo.value) return cloudStore.downloadUrl(props.item.path)
  return null
})

const downloadHref = computed(() => cloudStore.downloadUrl(props.item.path))
const { playing: audioPlaying, currentSrc } = useAudioPlayer()
const isCurrentlyPlaying = computed(() => audioPlaying.value && currentSrc.value === downloadHref.value)

// Uniform card cover ratio across every file type so folders, images, videos
// and documents all render at the same height in the grid (previously images/
// videos were square while folders were 4/3, giving a ragged, mismatched grid).
// Aspect is now driven entirely by .cloud-grid-card-cover CSS (4/3 desktop,
// square on mobile) so the ratio is deterministic regardless of Tailwind layer
// ordering.

const coverBg = computed(() => {
  if (props.item.isDir) return 'bg-amber-500/10'
  if (isAudio.value) return 'bg-orange-500/10'
  if (isVideo.value) return 'bg-purple-500/10'
  if (isImage.value) return 'bg-blue-500/10'
  if (category.value === 'document') return 'bg-green-500/10'
  if (category.value === 'spreadsheet') return 'bg-emerald-500/10'
  if (category.value === 'archive') return 'bg-yellow-500/10'
  return 'bg-white/5'
})

function handleClick() {
  if (props.item.isDir) {
    emit('navigate', props.item.path)
  } else if (isImage.value || isVideo.value) {
    emit('preview', props.item.path)
  }
}
</script>

<style scoped>
.cloud-card-menu { position: absolute; bottom: 8px; right: 8px; z-index: 4; }
.cloud-grid-card-info { padding-right: 60px; }
</style>
