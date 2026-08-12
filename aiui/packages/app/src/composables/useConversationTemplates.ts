import { ref, computed } from 'vue'

export interface ConversationTemplate {
  id: string
  title: string
  description: string
  systemPrompt: string
  firstMessage: string
  icon: string
  category: string
  model?: string
}

const STORAGE_KEY = 'aiui-conversation-templates'

const builtInTemplates: ConversationTemplate[] = [
  {
    id: 'tpl-bitcoin-deep-dive',
    title: 'Bitcoin Deep Dive',
    description: 'Explore Bitcoin technology, economics, and philosophy in depth.',
    systemPrompt: 'You are a knowledgeable Bitcoin educator. Explain concepts clearly, reference primary sources (whitepaper, BIPs), and maintain a cypherpunk perspective. Focus on sovereignty, decentralization, and sound money principles.',
    firstMessage: 'I want to understand Bitcoin at a deeper level. Can you start by explaining how proof-of-work creates trustless consensus?',
    icon: '₿',
    category: 'Bitcoin',
  },
  {
    id: 'tpl-film-analysis',
    title: 'Film Analysis',
    description: 'Analyze films through the lens of cinematography, narrative, and themes.',
    systemPrompt: 'You are a film critic and scholar. Discuss films with attention to cinematography, direction, narrative structure, themes, and cultural context. Reference specific scenes and techniques.',
    firstMessage: 'Let\'s analyze a film together. I\'d like to discuss the visual storytelling in Blade Runner 2049.',
    icon: '🎬',
    category: 'Creative',
  },
  {
    id: 'tpl-nostr-onboarding',
    title: 'Nostr Onboarding',
    description: 'Get started with the Nostr protocol and decentralized social media.',
    systemPrompt: 'You are a Nostr protocol expert. Help users understand key concepts: keypairs, relays, NIPs, clients, and the ecosystem. Be encouraging and practical.',
    firstMessage: 'I\'m new to Nostr. Can you explain what it is and how I can get started?',
    icon: '🔑',
    category: 'Technology',
  },
  {
    id: 'tpl-music-discovery',
    title: 'Music Discovery',
    description: 'Discover new music based on your tastes and explore genres.',
    systemPrompt: 'You are a music curator with deep knowledge across all genres. Recommend music based on user preferences, explain what makes artists and albums special, and connect musical lineages.',
    firstMessage: 'I love math rock and post-rock. What are some artists I should check out that push the boundaries of these genres?',
    icon: '🎵',
    category: 'Creative',
  },
  {
    id: 'tpl-code-review',
    title: 'Code Review',
    description: 'Get constructive feedback on your code with best practices.',
    systemPrompt: 'You are a senior software engineer conducting code reviews. Focus on readability, performance, security, and maintainability. Be constructive and specific.',
    firstMessage: 'I\'d like you to review some code I\'m working on. I\'ll paste it in the next message.',
    icon: '💻',
    category: 'Technology',
  },
  {
    id: 'tpl-privacy-guide',
    title: 'Privacy & Security Guide',
    description: 'Learn about digital privacy, operational security, and freedom tech.',
    systemPrompt: 'You are a digital privacy expert. Help users improve their online privacy and security. Recommend open-source tools, explain threat models, and promote self-sovereign digital identity.',
    firstMessage: 'I want to improve my digital privacy. Where should I start?',
    icon: '🛡️',
    category: 'Privacy',
  },
]

const customTemplates = ref<ConversationTemplate[]>([])

function loadCustomTemplates() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored) customTemplates.value = JSON.parse(stored)
  } catch { /* ignore */ }
}

function saveCustomTemplates() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(customTemplates.value))
}

loadCustomTemplates()

export function useConversationTemplates() {
  const allTemplates = computed(() => [...builtInTemplates, ...customTemplates.value])

  const categories = computed(() => {
    const cats = new Set(allTemplates.value.map((t) => t.category))
    return [...cats].sort()
  })

  function addTemplate(template: Omit<ConversationTemplate, 'id'>) {
    const newTemplate: ConversationTemplate = {
      ...template,
      id: `tpl-custom-${crypto.randomUUID()}`,
    }
    customTemplates.value.push(newTemplate)
    saveCustomTemplates()
    return newTemplate
  }

  function removeTemplate(id: string) {
    customTemplates.value = customTemplates.value.filter((t) => t.id !== id)
    saveCustomTemplates()
  }

  function exportTemplates(): string {
    return JSON.stringify(allTemplates.value, null, 2)
  }

  function importTemplates(json: string): number {
    try {
      const imported = JSON.parse(json) as ConversationTemplate[]
      if (!Array.isArray(imported)) return 0
      let count = 0
      for (const t of imported) {
        if (t.title && t.systemPrompt && t.firstMessage) {
          const exists = allTemplates.value.some((e) => e.id === t.id)
          if (!exists) {
            customTemplates.value.push({
              ...t,
              id: t.id || `tpl-imported-${crypto.randomUUID()}`,
            })
            count++
          }
        }
      }
      if (count > 0) saveCustomTemplates()
      return count
    } catch {
      return 0
    }
  }

  function getByCategory(category: string): ConversationTemplate[] {
    return allTemplates.value.filter((t) => t.category === category)
  }

  return {
    templates: allTemplates,
    categories,
    customTemplates,
    builtInTemplates,
    addTemplate,
    removeTemplate,
    exportTemplates,
    importTemplates,
    getByCategory,
  }
}
