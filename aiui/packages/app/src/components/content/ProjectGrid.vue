<template>
  <div class="flex flex-col h-full">
    <!-- Header with breadcrumb -->
    <div class="shrink-0 px-4 py-3 flex items-center justify-between gap-2"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <div class="flex items-center gap-1 min-w-0 flex-1">
        <!-- Breadcrumb: "Projects" root (clickable when deeper) -->
        <button
          v-if="viewState !== 'projects'"
          class="text-xs shrink-0 transition-colors"
          :class="isDark
            ? 'text-white/40 hover:text-white/70 hover:underline'
            : 'text-gray-400 hover:text-gray-700 hover:underline'"
          @click="backToProjects"
        >
          Projects
        </button>
        <span v-if="viewState !== 'projects'" class="text-xs shrink-0"
              :class="isDark ? 'text-white/20' : 'text-gray-300'">/</span>

        <!-- Current location (not clickable) -->
        <span class="text-sm font-semibold truncate"
              :class="isDark ? 'text-white/90' : 'text-gray-900'">
          {{ currentTitle }}
        </span>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <!-- Subtitle info -->
        <p v-if="viewState === 'projects'" class="text-xs"
           :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ projectList.length }} repos
        </p>
        <p v-else-if="viewState === 'filetree'" class="text-xs"
           :class="isDark ? 'text-white/30' : 'text-gray-400'">
          {{ activeProject?.language }}
        </p>
        <slot name="header-actions" />
      </div>
    </div>

    <!-- Project list -->
    <div v-if="viewState === 'projects'" class="flex-1 overflow-y-auto custom-scrollbar p-3">
      <!-- Search + New Project -->
      <div class="mb-3 flex gap-2">
        <input
          v-model="search"
          type="text"
          placeholder="Search projects..."
          class="flex-1 min-w-0 px-3 py-2 rounded-lg text-base bg-transparent outline-none"
          :class="isDark
            ? 'text-white/80 placeholder:text-white/20 border border-white/10 focus:border-white/25'
            : 'text-gray-800 placeholder:text-gray-400 border border-black/10 focus:border-black/20'"
        />
        <button
          class="shrink-0 px-3 py-2 rounded-lg text-xs font-medium transition-colors flex items-center gap-1.5"
          :class="isDark
            ? 'bg-accent/20 text-accent hover:bg-accent/30'
            : 'bg-accent/10 text-accent hover:bg-accent/20'"
          @click="showNewProjectDialog"
        >
          <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          New
        </button>
      </div>

      <!-- New project inline dialog -->
      <div v-if="isCreatingProject" class="mb-3 p-3 rounded-xl"
           :class="isDark ? 'bg-white/[0.05] border border-white/10' : 'bg-black/[0.03] border border-black/8'">
        <p class="text-xs font-medium mb-2"
           :class="isDark ? 'text-white/70' : 'text-gray-700'">
          New Project
        </p>
        <input
          ref="newProjectInputRef"
          v-model="newProjectName"
          type="text"
          placeholder="Project name..."
          class="w-full px-3 py-2 rounded-lg text-base bg-transparent outline-none mb-2"
          :class="isDark
            ? 'text-white/80 placeholder:text-white/20 border border-white/10 focus:border-white/25'
            : 'text-gray-800 placeholder:text-gray-400 border border-black/10 focus:border-black/20'"
          @keydown.enter="confirmCreateProject"
          @keydown.escape="cancelCreateProject"
        />
        <div class="flex justify-end gap-2">
          <button
            class="text-xs px-2.5 py-1 rounded-lg transition-colors"
            :class="isDark ? 'text-white/40 hover:text-white/70' : 'text-gray-500 hover:text-gray-800'"
            @click="cancelCreateProject"
          >
            Cancel
          </button>
          <button
            class="text-xs px-2.5 py-1 rounded-lg font-medium transition-colors"
            :class="isDark ? 'bg-accent/20 text-accent hover:bg-accent/30' : 'bg-accent/10 text-accent hover:bg-accent/20'"
            :disabled="!newProjectName.trim()"
            @click="confirmCreateProject"
          >
            Create
          </button>
        </div>
      </div>

      <!-- Project grid -->
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="project in filteredProjects"
          :key="project.path"
          class="text-left p-3 rounded-xl transition-all duration-150"
          :class="isDark
            ? 'bg-white/[0.03] hover:bg-white/[0.07] border border-white/5'
            : 'bg-black/[0.02] hover:bg-black/[0.05] border border-black/5'"
          @click="selectProject(project)"
        >
          <div class="w-8 h-8 rounded-lg flex items-center justify-center mb-2"
               :class="isDark ? 'bg-white/5' : 'bg-black/5'">
            <svg class="w-4 h-4" :class="project.isGit ? 'text-accent' : isDark ? 'text-white/40' : 'text-gray-400'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
          </div>
          <p class="text-xs font-medium truncate"
             :class="isDark ? 'text-white/80' : 'text-gray-800'">
            {{ project.name }}
          </p>
          <p class="text-xs mt-0.5 truncate"
             :class="isDark ? 'text-white/25' : 'text-gray-400'">
            {{ project.language }}
          </p>
        </button>
      </div>
    </div>

    <!-- File tree -->
    <div v-else-if="viewState === 'filetree'" class="flex-1 overflow-y-auto custom-scrollbar p-2">
      <FileTreeNode
        v-for="entry in fileTree"
        :key="entry.path"
        :entry="entry"
        :active-file="activeFile"
        :depth="0"
        @select="handleFileOpen"
        @toggle-context="handleToggleContext"
      />
      <div v-if="fileTree.length === 0" class="flex items-center justify-center py-12">
        <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
          Loading file tree...
        </p>
      </div>
    </div>

  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, onMounted } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useCodeContext, type ProjectInfo } from '@/composables/useCodeContext'
import FileTreeNode from './FileTreeNode.vue'

defineProps<{
  isWideDesktop?: boolean
  isMobile?: boolean
}>()

const { isDark } = useTheme()
const {
  projectList,
  activeProject,
  fileTree,
  activeFile,
  codeMode,
  selectedFiles,
  selectProject: doSelectProject,
  openFile,
  createProject,
  clearActiveFile,
  toggleFileSelection,
  isFileSelected,
  loadProjects,
} = useCodeContext()

onMounted(() => {
  if (projectList.value.length === 0) loadProjects()
})

const search = ref('')
const isCreatingProject = ref(false)
const newProjectName = ref('')
const newProjectInputRef = ref<HTMLInputElement | null>(null)

// View state: projects | filetree (file content always opens in Context panel)
type CodeViewState = 'projects' | 'filetree'
const viewState = computed<CodeViewState>(() => {
  if (!activeProject.value) return 'projects'
  return 'filetree'
})

const currentTitle = computed(() => {
  if (viewState.value === 'projects') return 'Projects'
  return activeProject.value?.name ?? 'Projects'
})

const filteredProjects = computed(() => {
  const q = search.value.toLowerCase()
  if (!q) return projectList.value
  return projectList.value.filter(p =>
    p.name.toLowerCase().includes(q) || (p.language ?? '').toLowerCase().includes(q)
  )
})

function selectProject(project: ProjectInfo) {
  doSelectProject(project)
}

function backToProjects() {
  const { activeProject: ap, fileTree: ft } = useCodeContext()
  ap.value = null
  ft.value = []
  clearActiveFile()
}

function handleFileOpen(filePath: string) {
  openFile(filePath)
}

function handleToggleContext(filePath: string) {
  toggleFileSelection(filePath)
}

// New project dialog
function showNewProjectDialog() {
  isCreatingProject.value = true
  newProjectName.value = ''
  nextTick(() => newProjectInputRef.value?.focus())
}

function cancelCreateProject() {
  isCreatingProject.value = false
  newProjectName.value = ''
}

function confirmCreateProject() {
  const name = newProjectName.value.trim()
  if (!name) return
  createProject(name)
  isCreatingProject.value = false
  newProjectName.value = ''
}
</script>

