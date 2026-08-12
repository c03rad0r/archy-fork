import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

export interface Persona {
  id: string
  name: string
  systemPrompt: string
  modelPreference?: string
  accentColor?: string
  isDefault?: boolean
}

const STORAGE_KEY = 'aiui-personas'

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

function loadFromStorage(): Persona[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    return JSON.parse(raw)
  } catch {
    return []
  }
}

function saveToStorage(personas: Persona[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(personas))
}

export const usePersonaStore = defineStore('personas', () => {
  const personas = ref<Persona[]>(loadFromStorage())

  const defaultPersona = computed(() => personas.value.find(p => p.isDefault) ?? null)

  const sortedPersonas = computed(() =>
    [...personas.value].sort((a, b) => {
      if (a.isDefault && !b.isDefault) return -1
      if (!a.isDefault && b.isDefault) return 1
      return a.name.localeCompare(b.name)
    })
  )

  function persist() {
    saveToStorage(personas.value)
  }

  function addPersona(data: Omit<Persona, 'id'>): Persona {
    const persona: Persona = { ...data, id: generateId() }
    // If this is the first or marked default, clear other defaults
    if (persona.isDefault) {
      personas.value.forEach(p => { p.isDefault = false })
    }
    personas.value.push(persona)
    persist()
    return persona
  }

  function updatePersona(id: string, data: Partial<Omit<Persona, 'id'>>) {
    const p = personas.value.find(x => x.id === id)
    if (!p) return
    if (data.isDefault) {
      personas.value.forEach(x => { x.isDefault = false })
    }
    Object.assign(p, data)
    persist()
  }

  function deletePersona(id: string) {
    const idx = personas.value.findIndex(x => x.id === id)
    if (idx !== -1) {
      personas.value.splice(idx, 1)
      persist()
    }
  }

  function getPersona(id: string): Persona | undefined {
    return personas.value.find(x => x.id === id)
  }

  return {
    personas,
    sortedPersonas,
    defaultPersona,
    addPersona,
    updatePersona,
    deletePersona,
    getPersona,
  }
})
