<template>
  <div class="space-y-0.5">
    <div v-for="item in items" :key="item.path">
      <button
        class="w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors min-h-[32px]"
        :class="item.isDirectory
          ? 'hover:bg-white/5 text-white/70 hover:text-white/80'
          : 'hover:bg-white/8 text-white/60 hover:text-white/80'"
        @click="handleClick(item)"
      >
        <!-- Expand/collapse chevron for directories -->
        <svg
          v-if="item.isDirectory"
          class="w-3 h-3 text-white/30 shrink-0 transition-transform duration-150"
          :class="{ 'rotate-90': expanded.has(item.path) }"
          fill="currentColor"
          viewBox="0 0 20 20"
        >
          <path fill-rule="evenodd" d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z" clip-rule="evenodd" />
        </svg>
        <span v-else class="w-3 shrink-0" />

        <!-- File/folder icon -->
        <svg
          class="w-4 h-4 shrink-0"
          :class="iconColor(item)"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            :d="iconPath(item)"
          />
        </svg>

        <!-- Name -->
        <span class="text-sm truncate">{{ item.name }}</span>
      </button>

      <!-- Children (recursive) -->
      <div
        v-if="item.isDirectory && item.children?.length && expanded.has(item.path)"
        class="pl-4 ml-[18px] border-l border-white/5"
      >
        <FileTree
          :items="item.children"
          @select-file="$emit('selectFile', $event)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  children?: FileEntry[]
}

defineProps<{ items: FileEntry[] }>()

const expanded = ref<Set<string>>(new Set())

const emit = defineEmits<{ selectFile: [entry: FileEntry] }>()

function handleClick(item: FileEntry) {
  if (item.isDirectory) {
    const next = new Set(expanded.value)
    if (next.has(item.path)) {
      next.delete(item.path)
    } else {
      next.add(item.path)
    }
    expanded.value = next
  } else {
    emit('selectFile', item)
  }
}

const CODE_EXTS = new Set([
  'ts', 'tsx', 'js', 'jsx', 'vue', 'svelte', 'py', 'rs', 'go', 'java',
  'c', 'cpp', 'h', 'hpp', 'rb', 'php', 'swift', 'kt', 'cs', 'css',
  'scss', 'less', 'html', 'xml', 'yaml', 'yml', 'toml', 'json', 'sh',
  'bash', 'zsh', 'sql', 'md', 'mdx',
])
const IMAGE_EXTS = new Set([
  'png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'ico', 'bmp', 'avif',
])

function fileExt(name: string): string {
  return name.split('.').pop()?.toLowerCase() ?? ''
}

function iconColor(item: FileEntry): string {
  if (item.isDirectory) return 'text-yellow-500/70'
  const ext = fileExt(item.name)
  if (CODE_EXTS.has(ext)) return 'text-blue-400/70'
  if (IMAGE_EXTS.has(ext)) return 'text-green-400/70'
  return 'text-white/40'
}

const FOLDER_PATH = 'M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z'
const FOLDER_OPEN_PATH = 'M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z'
const CODE_PATH = 'M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4'
const IMAGE_PATH = 'M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z'
const DOC_PATH = 'M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z'

function iconPath(item: FileEntry): string {
  if (item.isDirectory) {
    return expanded.value.has(item.path) ? FOLDER_OPEN_PATH : FOLDER_PATH
  }
  const ext = fileExt(item.name)
  if (CODE_EXTS.has(ext)) return CODE_PATH
  if (IMAGE_EXTS.has(ext)) return IMAGE_PATH
  return DOC_PATH
}
</script>
