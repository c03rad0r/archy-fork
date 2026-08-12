<template>
  <div v-if="isOpen" class="absolute bottom-full left-2 right-2 mb-1 z-50">
    <!-- Variable fill form -->
    <div
      v-if="selectedTemplate && variables.length > 0"
      class="rounded-2xl bg-[#1a1a1a] border border-white/10 shadow-2xl p-4 space-y-3 animate-scale-in"
    >
      <div class="flex items-center justify-between">
        <h4 class="text-xs font-semibold text-white/80">{{ selectedTemplate.title }}</h4>
        <button
          class="text-xs text-white/40 hover:text-white/60 transition-colors"
          @click="cancelTemplate"
        >
          Cancel
        </button>
      </div>
      <div v-for="v in variables" :key="v" class="space-y-1">
        <label class="text-xs text-white/40">{{ v }}</label>
        <input
          v-model="variableValues[v]"
          type="text"
          class="w-full bg-white/5 border border-white/10 rounded-lg px-3 py-1.5 text-base text-white/90 focus:outline-none focus:border-accent/50"
          :placeholder="v"
          @keydown.enter="applyTemplate"
        />
      </div>
      <button
        class="w-full py-1.5 rounded-lg text-xs bg-accent/20 text-accent hover:bg-accent/30 transition-all"
        @click="applyTemplate"
      >
        Insert
      </button>
    </div>

    <!-- Command + template list -->
    <div
      v-else
      class="rounded-2xl bg-[#1a1a1a] border border-white/10 shadow-2xl max-h-72 overflow-y-auto animate-scale-in"
    >
      <!-- Commands section -->
      <div v-if="filteredCommands.length > 0">
        <div class="p-2 border-b border-white/5">
          <p class="text-xs text-white/30 px-2">Commands</p>
        </div>
        <div
          v-for="(cmd, i) in filteredCommands"
          :key="cmd.id"
          class="px-3 py-2 cursor-pointer transition-colors"
          :class="i === highlightIndex ? 'bg-white/10' : 'hover:bg-white/5'"
          @click="selectCommand(cmd)"
          @mouseenter="highlightIndex = i"
        >
          <div class="flex items-center gap-2">
            <span class="text-xs font-mono text-accent/70">{{ cmd.slash }}</span>
            <p class="text-sm text-white/80">{{ cmd.title }}</p>
          </div>
          <p v-if="cmd.preview" class="text-xs text-white/40 mt-0.5 truncate">{{ cmd.preview }}</p>
        </div>
      </div>

      <!-- Templates section -->
      <div v-if="filteredUserTemplates.length > 0">
        <div class="p-2 border-b border-white/5">
          <p class="text-xs text-white/30 px-2">Templates</p>
        </div>
        <div
          v-for="(t, i) in filteredUserTemplates"
          :key="t.id"
          class="px-3 py-2 cursor-pointer transition-colors"
          :class="(filteredCommands.length + i) === highlightIndex ? 'bg-white/10' : 'hover:bg-white/5'"
          @click="selectTemplate(t)"
          @mouseenter="highlightIndex = filteredCommands.length + i"
        >
          <p class="text-sm text-white/80">{{ t.title }}</p>
          <p v-if="t.preview" class="text-xs text-white/40 mt-0.5 truncate">{{ t.preview }}</p>
        </div>
      </div>

      <div v-if="filteredCommands.length === 0 && filteredUserTemplates.length === 0" class="px-3 py-4 text-center">
        <p class="text-xs text-white/30">No matching commands</p>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { usePromptTemplateStore, extractVariables, type PromptTemplate } from '@/stores/promptTemplates'
import { DEMO_CONTENT_ENABLED } from '@/utils/demoContent'

interface PaletteCommand {
  id: string
  slash: string
  title: string
  preview: string
}

const BUILT_IN_COMMANDS: PaletteCommand[] = [
  { id: 'cmd-code', slash: '/code', title: 'Code', preview: 'Open project browser and code editor' },
  { id: 'cmd-nostr', slash: '/nostr', title: 'Nostr', preview: 'Browse the Nostr network feed' },
  { id: 'cmd-design', slash: '/design', title: 'Design System', preview: 'Open the design system viewer' },
  { id: 'cmd-search', slash: '/search ', title: 'Search', preview: 'Search your content library' },
  // /seed loads fabricated showcase content — demo-site/dev builds only (S7)
  ...(DEMO_CONTENT_ENABLED
    ? [{ id: 'cmd-seed', slash: '/seed', title: 'Seed', preview: 'Load seed conversations for all content types' }]
    : []),
  { id: 'cmd-freefilms', slash: '/freefilms', title: 'Free Films', preview: 'Browse free documentary films from InDeeHub' },
]

const props = defineProps<{
  query: string
  isOpen: boolean
}>()

const emit = defineEmits<{
  select: [text: string]
  close: []
}>()

const templateStore = usePromptTemplateStore()
const highlightIndex = ref(0)
const selectedTemplate = ref<PromptTemplate | null>(null)
const variableValues = ref<Record<string, string>>({})

const filteredCommands = computed(() => {
  const q = props.query.toLowerCase()
  if (!q) return BUILT_IN_COMMANDS
  return BUILT_IN_COMMANDS.filter(c =>
    c.slash.toLowerCase().includes('/' + q) ||
    c.title.toLowerCase().includes(q) ||
    c.preview.toLowerCase().includes(q)
  )
})

const filteredUserTemplates = computed(() => {
  const q = props.query.toLowerCase()
  if (!q) return templateStore.sortedTemplates
  return templateStore.sortedTemplates.filter(t =>
    t.title.toLowerCase().includes(q) || (t.preview ?? '').toLowerCase().includes(q)
  )
})

const allItemsCount = computed(() => filteredCommands.value.length + filteredUserTemplates.value.length)

const variables = computed(() => {
  if (!selectedTemplate.value) return []
  return extractVariables(selectedTemplate.value.content)
})

watch(() => props.query, () => {
  highlightIndex.value = 0
  selectedTemplate.value = null
})

watch(() => props.isOpen, (open) => {
  if (!open) {
    selectedTemplate.value = null
    variableValues.value = {}
    highlightIndex.value = 0
  }
})

function selectCommand(cmd: PaletteCommand) {
  // Commands like /code, /nostr send the slash text directly
  emit('select', cmd.slash)
}

function selectTemplate(t: PromptTemplate) {
  const vars = extractVariables(t.content)
  if (vars.length === 0) {
    emit('select', t.content)
    return
  }
  selectedTemplate.value = t
  variableValues.value = Object.fromEntries(vars.map(v => [v, '']))
}

function applyTemplate() {
  if (!selectedTemplate.value) return
  let result = selectedTemplate.value.content
  for (const [key, val] of Object.entries(variableValues.value)) {
    result = result.replaceAll(`{{${key}}}`, val || key)
  }
  emit('select', result)
  selectedTemplate.value = null
  variableValues.value = {}
}

function cancelTemplate() {
  selectedTemplate.value = null
  variableValues.value = {}
}

function navigateUp() {
  if (highlightIndex.value > 0) highlightIndex.value--
}

function navigateDown() {
  if (highlightIndex.value < allItemsCount.value - 1) highlightIndex.value++
}

function selectHighlighted() {
  const idx = highlightIndex.value
  if (idx < filteredCommands.value.length) {
    selectCommand(filteredCommands.value[idx])
  } else {
    const t = filteredUserTemplates.value[idx - filteredCommands.value.length]
    if (t) selectTemplate(t)
  }
}

defineExpose({
  navigateUp,
  navigateDown,
  selectHighlighted,
  hasSelectedTemplate: computed(() => !!selectedTemplate.value),
})
</script>
