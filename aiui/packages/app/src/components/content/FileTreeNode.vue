<template>
  <div class="group/node">
    <div
      class="w-full flex items-center gap-1.5 py-1 px-2 rounded-lg text-xs transition-colors cursor-pointer"
      :class="[
        isActive
          ? isDark ? 'bg-white/10 text-white/90' : 'bg-black/8 text-gray-900'
          : isDark ? 'text-white/60 hover:bg-white/[0.04] hover:text-white/80' : 'text-gray-600 hover:bg-black/[0.03] hover:text-gray-800',
      ]"
      :style="{ paddingLeft: `${depth * 12 + 8}px` }"
      @click="handleClick"
    >
      <!-- Expand/collapse for directories -->
      <svg
        v-if="entry.isDirectory"
        class="w-3 h-3 shrink-0 transition-transform duration-150"
        :class="expanded ? 'rotate-90' : ''"
        fill="none" stroke="currentColor" viewBox="0 0 24 24"
      >
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
      </svg>

      <!-- File/folder icon -->
      <svg class="w-3.5 h-3.5 shrink-0"
           :class="entry.isDirectory
             ? 'text-accent/70'
             : isDark ? 'text-white/30' : 'text-gray-400'"
           fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path v-if="entry.isDirectory" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
              d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
        <path v-else stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
      </svg>

      <span class="truncate flex-1">{{ entry.name }}</span>

      <!-- Context selector checkbox (files: far right, visible on hover or when selected) -->
      <button
        v-if="!entry.isDirectory"
        class="shrink-0 w-4 h-4 rounded-full border flex items-center justify-center transition-all ml-auto"
        :class="[
          isSelected
            ? 'bg-accent border-accent text-white'
            : isDark
              ? 'border-white/20 opacity-0 group-hover/node:opacity-100 hover:border-white/40'
              : 'border-black/15 opacity-0 group-hover/node:opacity-100 hover:border-black/30',
        ]"
        aria-label="Toggle file for chat context"
        @click.stop="handleToggleContext"
      >
        <svg v-if="isSelected" class="w-2.5 h-2.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
        </svg>
      </button>

      <!-- Context selector for directories (top-right, visible on hover or when selected) -->
      <button
        v-if="entry.isDirectory"
        class="shrink-0 w-4 h-4 rounded-full border flex items-center justify-center transition-all ml-auto"
        :class="[
          isDirSelected
            ? 'bg-accent border-accent text-white'
            : isDark
              ? 'border-white/20 opacity-0 group-hover/node:opacity-100 hover:border-white/40'
              : 'border-black/15 opacity-0 group-hover/node:opacity-100 hover:border-black/30',
        ]"
        aria-label="Add folder to chat context"
        @click.stop="handleToggleDirContext"
      >
        <svg v-if="isDirSelected" class="w-2.5 h-2.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
        </svg>
      </button>
    </div>

    <!-- Children (when expanded) -->
    <div v-if="entry.isDirectory && expanded && entry.children">
      <FileTreeNode
        v-for="child in entry.children"
        :key="child.path"
        :entry="child"
        :active-file="activeFile"
        :depth="depth + 1"
        @select="$emit('select', $event)"
        @toggle-context="$emit('toggle-context', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useCodeContext, type FileEntry } from '@/composables/useCodeContext'

const props = defineProps<{
  entry: FileEntry
  activeFile: string | null
  depth: number
}>()

const { isFileSelected } = useCodeContext()

const emit = defineEmits<{
  select: [path: string]
  'toggle-context': [path: string]
}>()

const { isDark } = useTheme()
const expanded = ref(props.depth < 1) // Auto-expand first level

const isActive = computed(() => !props.entry.isDirectory && props.activeFile === props.entry.path)
const isSelected = computed(() => !props.entry.isDirectory && isFileSelected(props.entry.path))
const isDirSelected = computed(() => props.entry.isDirectory && isFileSelected(props.entry.path))

function handleClick() {
  if (props.entry.isDirectory) {
    expanded.value = !expanded.value
  } else {
    // Click opens file in code viewer
    emit('select', props.entry.path)
  }
}

function handleToggleContext() {
  emit('toggle-context', props.entry.path)
}

function handleToggleDirContext() {
  emit('toggle-context', props.entry.path)
}
</script>
