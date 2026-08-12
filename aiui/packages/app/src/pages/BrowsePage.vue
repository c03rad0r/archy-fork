<template>
  <div class="h-full flex flex-col bg-[#0a0a0a]">
    <!-- Header -->
    <header class="glass shrink-0 px-4 py-3 flex items-center gap-3 border-b border-white/5">
      <router-link
        to="/"
        class="min-w-[44px] min-h-[44px] flex items-center justify-center rounded-lg text-white/60 hover:text-white/80 hover:bg-white/10 transition-colors"
        aria-label="Back to chat"
      >
        <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </router-link>
      <h1 class="text-white/90 text-base font-medium truncate">Files</h1>
    </header>

    <!-- Breadcrumb -->
    <nav class="px-4 py-2 flex items-center gap-1 text-xs text-white/40 shrink-0">
      <button
        class="hover:text-white/70 transition-colors min-h-[28px] px-1"
        @click="backToProjects"
      >
        Projects
      </button>
      <template v-if="currentProject">
        <span class="text-white/20">/</span>
        <span class="text-white/60 min-h-[28px] px-1 flex items-center">
          {{ currentProject.name }}
        </span>
      </template>
    </nav>

    <!-- Content -->
    <main class="flex-1 overflow-hidden flex">
      <!-- File tree / project list -->
      <div class="flex-1 overflow-y-auto px-2 py-2">
        <div v-if="loading" class="flex items-center justify-center h-32">
          <span class="text-sm text-white/50">Loading...</span>
        </div>
        <div v-else-if="error" class="flex items-center justify-center h-32">
          <span class="text-sm text-red-400/70">{{ error }}</span>
        </div>

        <!-- Project list -->
        <div v-else-if="!currentProject" class="space-y-1">
          <button
            v-for="project in projects"
            :key="project.path"
            class="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-left hover:bg-white/5 transition-colors group"
            @click="openProject(project)"
          >
            <svg class="w-5 h-5 text-white/30 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
            <div class="min-w-0 flex-1">
              <p class="text-sm text-white/80 truncate group-hover:text-white/90">{{ project.name }}</p>
              <p class="text-xs text-white/25 truncate">{{ project.language }}</p>
            </div>
          </button>
        </div>

        <!-- File tree -->
        <FileTree
          v-else
          :items="treeItems"
          @select-file="openFile"
        />
      </div>

      <!-- File preview (desktop sidebar) -->
      <aside
        v-if="previewFile && !isMobile"
        class="w-[400px] xl:w-[500px] border-l border-white/5 overflow-y-auto shrink-0"
      >
        <FilePreview :file="previewFile" @close="previewFile = null" />
      </aside>
    </main>

    <!-- Mobile file preview overlay -->
    <Teleport to="body">
      <div
        v-if="previewFile && isMobile"
        class="fixed inset-0 z-50 bg-[#0a0a0a] overflow-y-auto"
      >
        <FilePreview :file="previewFile" @close="previewFile = null" />
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import FileTree from '@/components/browse/FileTree.vue'
import FilePreview from '@/components/browse/FilePreview.vue'
import { apiFetch } from '@/utils/api-fetch'

interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  children?: FileEntry[]
}

interface Project {
  name: string
  path: string
  language: string
}

interface PreviewData {
  name: string
  path: string
  content: string
  size: number
}

const projects = ref<Project[]>([])
const treeItems = ref<FileEntry[]>([])
const loading = ref(true)
const error = ref('')
const currentProject = ref<Project | null>(null)
const previewFile = ref<PreviewData | null>(null)

const windowWidth = ref(window.innerWidth)
const isMobile = computed(() => windowWidth.value < 1024)

async function loadProjects() {
  loading.value = true
  error.value = ''
  try {
    const res = await apiFetch('/api/fs/list')
    if (!res.ok) throw new Error(`Failed to load: ${res.status}`)
    const data = await res.json()
    projects.value = data.projects ?? []
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load projects'
  } finally {
    loading.value = false
  }
}

async function openProject(project: Project) {
  currentProject.value = project
  previewFile.value = null
  loading.value = true
  error.value = ''
  try {
    const res = await apiFetch(`/api/fs/tree?path=${encodeURIComponent(project.path)}`)
    if (!res.ok) throw new Error(`Failed to load: ${res.status}`)
    const data = await res.json()
    treeItems.value = data.files ?? []
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to load files'
  } finally {
    loading.value = false
  }
}

function backToProjects() {
  currentProject.value = null
  previewFile.value = null
  treeItems.value = []
}

async function openFile(entry: FileEntry) {
  if (!currentProject.value) return
  const absolutePath = currentProject.value.path + '/' + entry.path
  try {
    const res = await apiFetch(`/api/fs/read?path=${encodeURIComponent(absolutePath)}`)
    if (!res.ok) {
      if (res.status === 413) {
        error.value = 'File too large to preview (max 1MB)'
        return
      }
      throw new Error(`Failed to read: ${res.status}`)
    }
    const data = await res.json()
    previewFile.value = {
      name: entry.name,
      path: entry.path,
      content: data.content,
      size: data.size,
    }
  } catch (e) {
    error.value = e instanceof Error ? e.message : 'Failed to read file'
  }
}

function onResize() { windowWidth.value = window.innerWidth }

onMounted(() => {
  loadProjects()
  window.addEventListener('resize', onResize)
})

onUnmounted(() => {
  window.removeEventListener('resize', onResize)
})
</script>
