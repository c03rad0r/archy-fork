import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface PromptTemplate {
  id: string
  title: string
  content: string
  /** Preview text shown in palette */
  preview?: string
}

const STORAGE_KEY = 'aiui-prompt-templates'

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

const DEFAULT_TEMPLATES: PromptTemplate[] = [
  {
    id: 'builtin-explain',
    title: 'Explain like I\'m 5',
    content: 'Explain {{topic}} in simple terms that a 5-year-old would understand.',
    preview: 'Simplify complex topics',
  },
  {
    id: 'builtin-compare',
    title: 'Compare & Contrast',
    content: 'Compare and contrast {{option A}} and {{option B}}. List pros, cons, and a recommendation.',
    preview: 'Side-by-side analysis',
  },
  {
    id: 'builtin-summarise',
    title: 'Summarise',
    content: 'Summarise the following in {{length}} bullet points:\n\n{{text}}',
    preview: 'Condense text to key points',
  },
  {
    id: 'builtin-translate',
    title: 'Translate',
    content: 'Translate the following to {{language}}:\n\n{{text}}',
    preview: 'Translate text to another language',
  },
]

function loadFromStorage(): PromptTemplate[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return [...DEFAULT_TEMPLATES]
    return JSON.parse(raw)
  } catch {
    return [...DEFAULT_TEMPLATES]
  }
}

function saveToStorage(templates: PromptTemplate[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(templates))
}

/** Extract variable names from template content */
export function extractVariables(content: string): string[] {
  const matches = content.match(/\{\{([^}]+)\}\}/g)
  if (!matches) return []
  return [...new Set(matches.map(m => m.slice(2, -2).trim()))]
}

export const usePromptTemplateStore = defineStore('promptTemplates', () => {
  const templates = ref<PromptTemplate[]>(loadFromStorage())

  const sortedTemplates = computed(() =>
    [...templates.value].sort((a, b) => a.title.localeCompare(b.title))
  )

  function persist() {
    saveToStorage(templates.value)
  }

  function addTemplate(data: Omit<PromptTemplate, 'id'>): PromptTemplate {
    const template: PromptTemplate = { ...data, id: generateId() }
    templates.value.push(template)
    persist()
    return template
  }

  function updateTemplate(id: string, data: Partial<Omit<PromptTemplate, 'id'>>) {
    const t = templates.value.find(x => x.id === id)
    if (!t) return
    Object.assign(t, data)
    persist()
  }

  function deleteTemplate(id: string) {
    const idx = templates.value.findIndex(x => x.id === id)
    if (idx !== -1) {
      templates.value.splice(idx, 1)
      persist()
    }
  }

  function exportTemplates(): string {
    return JSON.stringify(templates.value, null, 2)
  }

  function importTemplates(json: string): number {
    try {
      const parsed = JSON.parse(json) as PromptTemplate[]
      if (!Array.isArray(parsed)) return 0
      let count = 0
      for (const t of parsed) {
        if (t.title && t.content && !templates.value.find(x => x.id === t.id)) {
          templates.value.push({ id: t.id || generateId(), title: t.title, content: t.content, preview: t.preview })
          count++
        }
      }
      persist()
      return count
    } catch {
      return 0
    }
  }

  return {
    templates,
    sortedTemplates,
    addTemplate,
    updateTemplate,
    deleteTemplate,
    exportTemplates,
    importTemplates,
  }
})
