<template>
  <button
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

      <!-- Actions overlay at top-left (visible on hover) -->
      <div class="cloud-grid-card-actions" @click.stop>
        <button
          class="cloud-file-action-btn cloud-file-action-share"
          title="Share with peers"
          @click.stop="emit('share', item.path, item.name, item.isDir)"
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
          @click.stop="emit('delete', item.path)"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
          </svg>
        </button>
      </div>
    </div>
  </button>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
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
