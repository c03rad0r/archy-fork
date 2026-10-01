<template>
  <div class="cloud-folder-container flex flex-col h-full">
    <!-- Desktop Back Button + Header -->
    <div class="shrink-0 mb-4">
      <BackButton :label="backLabel" @click="goBack" />

      <!-- Folder Header -->
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-4">
          <div class="flex-shrink-0 w-12 h-12 rounded-xl flex items-center justify-center" :class="section?.iconBg || 'bg-white/10'">
            <svg class="w-7 h-7" :class="section?.iconColor || 'text-white/70'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                v-for="(path, index) in (section?.iconPaths || [])"
                :key="index"
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                :d="path"
              />
            </svg>
          </div>
          <div class="hidden md:block">
            <h1 class="text-2xl font-bold text-white">{{ section?.name || 'Folder' }}</h1>
            <p class="text-sm text-white/50">{{ section?.description }}</p>
          </div>
        </div>
        <div class="flex gap-2">
          <button
            v-if="appRunning"
            @click="openExternal"
            class="glass-button px-4 py-2 rounded-lg text-sm font-medium flex items-center gap-2"
          >
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
            </svg>
            Open in New Tab
          </button>
        </div>
      </div>
    </div>

    <UploadProgress />

    <!-- App Not Installed -->
    <div v-if="!appRunning" class="glass-card p-12 text-center flex-1 flex flex-col items-center justify-center">
      <svg class="w-20 h-20 text-white/15 mb-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
      <h3 class="text-xl font-semibold text-white mb-2">{{ section?.appLabel }} not running</h3>
      <p class="text-white/60 mb-4">Install {{ section?.appLabel }} from the App Store to manage your {{ section?.name?.toLowerCase() }}.</p>
      <RouterLink to="/dashboard/marketplace" class="glass-button inline-flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium">
        Open App Store
      </RouterLink>
    </div>

    <!-- Cloud Store Error -->
    <div v-else-if="cloudStore.error" class="glass-card p-6 flex-1 flex flex-col items-center justify-center text-center">
      <div class="alert-error mb-4">{{ cloudStore.error }}</div>
      <button class="glass-button px-4 py-2 rounded-lg text-sm" @click="cloudStore.refresh()">Retry</button>
    </div>

    <!-- Native File Browser (for FileBrowser-backed sections) -->
    <div
      v-else-if="useNativeUI"
      class="flex-1 min-h-0 flex flex-col relative"
      @dragover.prevent="onDragOver"
      @dragleave="onDragLeave"
      @drop.prevent="onDrop"
    >
      <!-- Drag-and-drop overlay -->
      <div v-if="draggingOver" class="cloud-drop-overlay">
        <div class="cloud-drop-overlay-inner">
          <svg class="w-12 h-12 text-white/80 mb-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
          </svg>
          <p class="text-lg font-medium text-white/90">Drop files to upload</p>
          <p class="text-sm text-white/50">Files will be added to the current folder</p>
        </div>
      </div>

      <CloudToolbar
        :breadcrumbs="cloudStore.breadcrumbs"
        :view-mode="viewMode"
        :uploading="!!cloudStore.upload?.active"
        @navigate="navigateCloudPath"
        @refresh="cloudStore.refresh()"
        @upload="handleUpload"
        @update:view-mode="viewMode = $event"
      />
      <!-- Re-key on the current folder path so the depth/zoom animation replays
           at every level (folder → subfolder → …), not just on first entry.
           The transition name flips with navigation direction so descending
           zooms forward and going back up zooms in reverse — matching the
           cloud → folder route transition. Only the file content zooms; the
           header + breadcrumb nav above stay fixed in place. -->
      <Transition :name="folderTransition" mode="out-in">
        <FileGrid
          :key="cloudStore.currentPath"
          :items="cloudStore.sortedItems"
          :loading="cloudStore.loading"
          :view-mode="viewMode"
        :uploading="!!cloudStore.upload?.active"
          @navigate="navigateCloudPath"
          @delete="handleDelete"
          @play="handlePlay"
          @share="handleShare"
          @preview="handlePreview"
        />
      </Transition>

      <!-- Audio player is now the global bottom bar (GlobalAudioPlayer in App.vue) -->
    </div>

    <!-- Fallback iframe (for sections without native UI) -->
    <div v-else class="flex-1 min-h-0 rounded-xl overflow-hidden border border-white/10">
      <div v-if="!iframeLoaded" class="flex items-center justify-center h-full">
        <div class="glass-card p-8 flex flex-col items-center gap-4">
          <div class="w-8 h-8 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
          <p class="text-sm text-white/60">Loading {{ section?.appLabel }}...</p>
        </div>
      </div>
      <iframe
        v-if="appRunning"
        :src="iframeUrl"
        class="w-full h-full border-0"
        :class="{ 'opacity-0': !iframeLoaded }"
        style="min-height: 500px"
        @load="iframeLoaded = true"
        sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
      />
    </div>

    <!-- Share Modal -->
    <ShareModal
      v-if="shareTarget"
      :filename="shareTarget.name"
      :filepath="shareTarget.path"
      :is-dir="shareTarget.isDir"
      @close="shareTarget = null"
      @saved="shareTarget = null"
    />

    <!-- Media Lightbox -->
    <MediaLightbox
      v-if="lightboxIndex !== null"
      :items="cloudStore.sortedItems"
      :start-index="lightboxIndex"
      :show="lightboxIndex !== null"
      :fetch-blob-url="cloudStore.fetchBlobUrl"
      :stream-url="cloudStore.streamUrl"
      @close="lightboxIndex = null"
    />
  </div>
</template>

<script setup lang="ts">
import UploadProgress from '@/components/cloud/UploadProgress.vue'
import { ref, computed, watch } from 'vue'
import { useRouter, useRoute, RouterLink } from 'vue-router'
import { useAppStore } from '../stores/app'
import { useCloudStore } from '../stores/cloud'
import BackButton from '@/components/BackButton.vue'
import CloudToolbar from '../components/cloud/CloudToolbar.vue'
import FileGrid from '../components/cloud/FileGrid.vue'
import ShareModal from '../components/cloud/ShareModal.vue'
import MediaLightbox from '../components/cloud/MediaLightbox.vue'
import { useAudioPlayer } from '../composables/useAudioPlayer'
import { getFileCategory } from '../composables/useFileType'
import { normalizeCloudPath, parentCloudPath } from './cloudPath'

const router = useRouter()
const route = useRoute()
const store = useAppStore()
const cloudStore = useCloudStore()
const viewMode = ref<'list' | 'grid'>('grid')
const audioPlayer = useAudioPlayer()

// Direction-aware folder zoom: descending into a subfolder plays the same
// "depth-forward" feel as the cloud → folder route transition (new arrives from
// depth, current zooms out toward the viewer); navigating back up plays its
// mirror ("depth-back"). Picked by comparing folder depth on each path change.
const folderTransition = ref<'cloud-zoom-forward' | 'cloud-zoom-back'>('cloud-zoom-forward')
let prevFolderDepth = -1
watch(() => cloudStore.currentPath, (path) => {
  const depth = path.split('/').filter(Boolean).length
  // First render (prevFolderDepth === -1) defaults to forward.
  folderTransition.value = depth < prevFolderDepth ? 'cloud-zoom-back' : 'cloud-zoom-forward'
  prevFolderDepth = depth
})

const iframeLoaded = ref(false)
const folderId = computed(() => route.params.folderId as string)
const routeFolderPath = computed(() => normalizeCloudPath(route.query.path, section.value?.initialPath || '/'))

const APP_ALIASES: Record<string, string[]> = {
  immich: ['immich_server', 'immich-server'],
  nextcloud: ['nextcloud-aio', 'nextcloud-server'],
}

function isAppRunning(appId: string): boolean {
  const packages = store.packages
  if (packages[appId]?.state === 'running') return true
  const aliases = APP_ALIASES[appId]
  if (aliases) {
    for (const alias of aliases) {
      if (packages[alias]?.state === 'running') return true
    }
  }
  return false
}

interface ContentSection {
  name: string
  description: string
  appId: string
  appLabel: string
  iconPaths: string[]
  iconBg: string
  iconColor: string
  iframeUrl: string
  externalUrl: string
  nativeUI: boolean
  initialPath: string
}

const origin = computed(() => window.location.origin)

const sections: Record<string, () => ContentSection> = {
  photos: () => ({
    name: 'Photos & Videos',
    description: 'Auto-backup & browse your media',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z'],
    iconBg: 'bg-blue-500/15',
    iconColor: 'text-blue-400',
    iframeUrl: `${origin.value}/app/immich/photos`,
    externalUrl: `${origin.value}/app/filebrowser/`,
    nativeUI: true,
    initialPath: '/Photos',
  }),
  music: () => ({
    name: 'Music',
    description: 'Your music collection',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3'],
    iconBg: 'bg-orange-500/15',
    iconColor: 'text-orange-400',
    iframeUrl: `${origin.value}/app/nextcloud/apps/files/?dir=/Songs`,
    externalUrl: `${origin.value}/app/filebrowser/`,
    nativeUI: true,
    initialPath: '/Music',
  }),
  documents: () => ({
    name: 'Documents',
    description: 'Files, docs & spreadsheets',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z'],
    iconBg: 'bg-green-500/15',
    iconColor: 'text-green-400',
    iframeUrl: `${origin.value}/app/nextcloud/apps/files/?dir=/Documents`,
    externalUrl: `${origin.value}/app/filebrowser/`,
    nativeUI: true,
    initialPath: '/Documents',
  }),
  files: () => ({
    name: 'All Files',
    description: 'Browse your server file system',
    appId: 'filebrowser',
    appLabel: 'File Browser',
    iconPaths: ['M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'],
    iconBg: 'bg-white/10',
    iconColor: 'text-white/70',
    iframeUrl: `${origin.value}/app/filebrowser/`,
    externalUrl: `${origin.value}/app/filebrowser/`,
    nativeUI: true,
    initialPath: '/',
  }),
}

const section = computed(() => {
  const factory = sections[folderId.value]
  return factory ? factory() : null
})

const appRunning = computed(() => section.value ? isAppRunning(section.value.appId) : false)
const useNativeUI = computed(() => section.value?.nativeUI === true && appRunning.value)
const iframeUrl = computed(() => section.value?.iframeUrl || '')
// Whether we're at the section's root folder. Derived from the route (the URL
// is the source of truth) rather than cloudStore.currentPath, which is async
// and still holds the previous/blank path on first entry — that staleness is
// what made entering e.g. "Photos and videos" wrongly show "Back to Parent
// Folder" and break the back action.
const atSectionRoot = computed(() =>
  !section.value || routeFolderPath.value === section.value.initialPath
)
const backLabel = computed(() => {
  if (!useNativeUI.value || !section.value) return 'Back to Cloud'
  return atSectionRoot.value ? 'Back to Cloud' : 'Back to Parent Folder'
})

// Initialize native file browser when entering a native-UI section.
// No reset() here: navigate() serves the per-path cache instantly and
// revalidates underneath — resetting wiped the listing and forced a
// spinner on every folder entry.
watch([useNativeUI, section, routeFolderPath], async ([native, sec, path]) => {
  if (native && sec) {
    const ok = await cloudStore.init()
    if (ok) {
      await cloudStore.navigate(path)
    }
  }
}, { immediate: true })

const shareTarget = ref<{ path: string; name: string; isDir: boolean } | null>(null)
const lightboxIndex = ref<number | null>(null)

function handlePreview(path: string) {
  const items = cloudStore.sortedItems
  // Audio never opens the lightbox — it belongs to the bottom-bar player.
  const clicked = items.find(item => item.path === path)
  if (clicked) {
    const ext = clicked.name.includes('.') ? clicked.name.split('.').pop()!.toLowerCase() : ''
    if (getFileCategory(ext, clicked.isDir) === 'audio') {
      void handlePlay(path, clicked.name)
      return
    }
  }
  // MediaLightbox internally filters items to media only, so startIndex
  // must be the index within that filtered list
  const mediaItems = items.filter(item => {
    const ext = item.name.includes('.') ? item.name.split('.').pop()!.toLowerCase() : ''
    const cat = getFileCategory(ext, item.isDir)
    return cat === 'image' || cat === 'video' || cat === 'audio'
  })
  const idx = mediaItems.findIndex(item => item.path === path)
  lightboxIndex.value = idx >= 0 ? idx : 0
}

function handleShare(path: string, name: string, isDir: boolean) {
  shareTarget.value = { path, name, isDir }
}

const draggingOver = ref(false)
let dragLeaveTimer: ReturnType<typeof setTimeout> | null = null

function onDragOver() {
  if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
  draggingOver.value = true
}

function onDragLeave() {
  // Debounce to avoid flicker when dragging between child elements
  if (dragLeaveTimer) clearTimeout(dragLeaveTimer)
  dragLeaveTimer = setTimeout(() => { draggingOver.value = false }, 100)
}

function onDrop(e: DragEvent) {
  draggingOver.value = false
  if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
  const dt = e.dataTransfer
  if (!dt?.files?.length) return
  handleUpload(Array.from(dt.files))
}

async function handleUpload(files: File[]) {
  await cloudStore.uploadFiles(files)
}

async function handleDelete(path: string) {
  await cloudStore.deleteItem(path)
}

async function handlePlay(path: string, name: string) {
  const url = await cloudStore.streamUrl(path)
  audioPlayer.play(url, name)
}

function openExternal() {
  if (section.value) {
    window.open(section.value.externalUrl, '_blank', 'noopener,noreferrer')
  }
}

async function navigateCloudPath(path: string) {
  const target = normalizeCloudPath(path, section.value?.initialPath || '/')
  await router.push({
    name: 'cloud-folder',
    params: { folderId: folderId.value },
    query: target === section.value?.initialPath ? {} : { path: target },
  })
}

function goBack() {
  if (useNativeUI.value && !atSectionRoot.value) {
    navigateCloudPath(parentCloudPath(routeFolderPath.value))
    return
  }
  router.push('/dashboard/cloud')
}
</script>

<!-- Not scoped: the transition classes are applied to the FileGrid child's root
     element, which lives outside this component's style scope. Mirrors the
     `depth-forward` / `depth-back` route transitions (same scale magnitudes +
     blur) so descending into a folder and going back up feel identical to the
     cloud ⇄ folder route change. -->
<style>
.cloud-zoom-forward-enter-active,
.cloud-zoom-forward-leave-active,
.cloud-zoom-back-enter-active,
.cloud-zoom-back-leave-active {
  transition:
    opacity 0.4s cubic-bezier(0.25, 0.46, 0.45, 0.94),
    transform 0.4s cubic-bezier(0.25, 0.46, 0.45, 0.94),
    filter 0.4s cubic-bezier(0.25, 0.46, 0.45, 0.94);
  transform-origin: center center;
  will-change: opacity, transform, filter;
}

/* Forward (into a deeper folder): new folder arrives from depth while the
   current one zooms out toward the viewer — matches depth-forward. */
.cloud-zoom-forward-enter-from {
  opacity: 0;
  transform: scale(0.75);
  filter: blur(4px);
}
.cloud-zoom-forward-leave-to {
  opacity: 0;
  transform: scale(1.2);
  filter: blur(8px);
}

/* Back (up to a parent folder): the mirror — new folder shrinks in from the
   front while the current one recedes into depth — matches depth-back. */
.cloud-zoom-back-enter-from {
  opacity: 0;
  transform: scale(1.2);
  filter: blur(8px);
}
.cloud-zoom-back-leave-to {
  opacity: 0;
  transform: scale(0.75);
  filter: blur(4px);
}

@media (prefers-reduced-motion: reduce) {
  .cloud-zoom-forward-enter-active,
  .cloud-zoom-forward-leave-active,
  .cloud-zoom-back-enter-active,
  .cloud-zoom-back-leave-active {
    transition: opacity 0.2s ease;
  }
  .cloud-zoom-forward-enter-from,
  .cloud-zoom-forward-leave-to,
  .cloud-zoom-back-enter-from,
  .cloud-zoom-back-leave-to {
    transform: none;
    filter: none;
  }
}
</style>
