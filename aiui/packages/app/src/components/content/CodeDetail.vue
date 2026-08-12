<template>
  <div class="code-detail h-full flex flex-col overflow-hidden"
       :class="isDark ? 'bg-[#1a1a2e]' : 'bg-[#fafafa]'">

    <!-- Header with file name + back button -->
    <div class="shrink-0 flex items-center gap-2 px-3 py-2"
         :style="isDark
           ? 'border-bottom: 1px solid rgba(255, 255, 255, 0.08)'
           : 'border-bottom: 1px solid rgba(0, 0, 0, 0.06)'">
      <button
        class="absolute top-3 left-3 p-2 rounded-lg path-glass-icon z-10 transition-colors hover:bg-white/10"
        @click="$emit('back')"
      >
        <svg class="w-4 h-4" :class="isDark ? 'text-white/70' : 'text-gray-600'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
      </button>

      <div class="flex-1 min-w-0 pl-8">
        <div class="flex items-center gap-2">
          <!-- Language badge -->
          <span class="shrink-0 text-xs px-1.5 py-0.5 rounded font-mono"
                :class="isDark ? 'bg-white/10 text-white/50' : 'bg-black/5 text-gray-500'">
            {{ language }}
          </span>
          <p class="text-xs font-mono truncate"
             :class="isDark ? 'text-white/70' : 'text-gray-700'">
            {{ fileName }}
          </p>
        </div>
        <p v-if="projectName" class="text-xs font-mono mt-0.5 truncate"
           :class="isDark ? 'text-white/25' : 'text-gray-400'">
          {{ projectName }} / {{ filePath }}
        </p>
      </div>
    </div>

    <!-- Code content -->
    <div class="flex-1 min-h-0 overflow-auto custom-scrollbar">
      <div v-if="content" class="font-mono text-xs leading-relaxed">
        <table class="w-full border-collapse">
          <tbody>
            <tr v-for="(line, i) in lines" :key="i"
                class="hover:bg-white/[0.03]">
              <td class="select-none text-right pr-4 pl-4 py-0 align-top w-1"
                  :class="isDark ? 'text-white/15' : 'text-gray-300'"
                  style="min-width: 3rem;">
                {{ i + 1 }}
              </td>
              <td class="pr-4 py-0 whitespace-pre"
                  :class="isDark ? 'text-white/75' : 'text-gray-700'">{{ line }}</td>
            </tr>
          </tbody>
        </table>
      </div>

      <!-- Empty state -->
      <div v-else class="flex items-center justify-center h-full">
        <div class="text-center space-y-3 px-6">
          <div class="w-16 h-16 rounded-2xl flex items-center justify-center mx-auto"
               :class="isDark ? 'bg-white/5' : 'bg-black/5'">
            <svg class="w-7 h-7" :class="isDark ? 'text-white/20' : 'text-gray-300'" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5"
                    d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
            </svg>
          </div>
          <p class="text-xs" :class="isDark ? 'text-white/30' : 'text-gray-400'">
            Select a file to view its contents.
          </p>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useTheme } from '@/composables/useTheme'
import { useCodeContext } from '@/composables/useCodeContext'

defineEmits<{
  back: []
}>()

const { isDark } = useTheme()
const { activeFile, activeFileContent, activeFileLanguage, activeProject } = useCodeContext()

const content = computed(() => activeFileContent.value)
const language = computed(() => activeFileLanguage.value)
const filePath = computed(() => activeFile.value ?? '')
const fileName = computed(() => filePath.value.split('/').pop() ?? '')
const projectName = computed(() => activeProject.value?.name ?? '')

const lines = computed(() => {
  if (!content.value) return []
  return content.value.split('\n')
})
</script>

<style scoped>
.code-detail table {
  font-variant-numeric: tabular-nums;
}
</style>
