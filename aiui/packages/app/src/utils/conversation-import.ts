import type { Conversation, Message } from '@aiui/core/types/message'

function generateId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID()
  }
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = (Math.random() * 16) | 0
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

interface ImportResult {
  conversations: Conversation[]
  format: 'aiui' | 'claude' | 'unknown'
  error?: string
}

/** Try to parse as AIUI JSON export (single conversation) */
function parseAIUIFormat(data: unknown): Conversation | null {
  if (!data || typeof data !== 'object') return null
  const obj = data as Record<string, unknown>

  if (typeof obj.id === 'string' && typeof obj.title === 'string' && Array.isArray(obj.messages)) {
    return obj as unknown as Conversation
  }
  return null
}

/** Try to parse Claude.ai export format */
function parseClaudeFormat(data: unknown): Conversation[] {
  if (!Array.isArray(data)) return []

  const conversations: Conversation[] = []
  for (const item of data) {
    if (!item || typeof item !== 'object') continue
    const obj = item as Record<string, unknown>

    // Claude.ai exports have { uuid, name, chat_messages: [...] }
    if (typeof obj.uuid === 'string' && typeof obj.name === 'string' && Array.isArray(obj.chat_messages)) {
      const messages: Message[] = []
      for (const cm of obj.chat_messages as Record<string, unknown>[]) {
        if (!cm || typeof cm !== 'object') continue
        const role = cm.sender === 'human' ? 'user' as const : 'assistant' as const
        const content = typeof cm.text === 'string' ? cm.text : ''
        messages.push({
          id: generateId(),
          role,
          content,
          timestamp: typeof cm.created_at === 'string' ? new Date(cm.created_at as string).getTime() : Date.now(),
        })
      }

      conversations.push({
        id: generateId(),
        title: obj.name as string,
        messages,
        createdAt: typeof obj.created_at === 'string' ? new Date(obj.created_at as string).getTime() : Date.now(),
        updatedAt: typeof obj.updated_at === 'string' ? new Date(obj.updated_at as string).getTime() : Date.now(),
      })
    }
  }
  return conversations
}

export function parseImportFile(jsonString: string): ImportResult {
  try {
    const data = JSON.parse(jsonString)

    // Try AIUI single conversation
    const aiui = parseAIUIFormat(data)
    if (aiui) {
      return { conversations: [aiui], format: 'aiui' }
    }

    // Try Claude.ai export (array of conversations)
    const claude = parseClaudeFormat(data)
    if (claude.length > 0) {
      return { conversations: claude, format: 'claude' }
    }

    // Try AIUI array format
    if (Array.isArray(data)) {
      const aiuiConvs: Conversation[] = []
      for (const item of data) {
        const c = parseAIUIFormat(item)
        if (c) aiuiConvs.push(c)
      }
      if (aiuiConvs.length > 0) {
        return { conversations: aiuiConvs, format: 'aiui' }
      }
    }

    return { conversations: [], format: 'unknown', error: 'Unrecognized format' }
  } catch {
    return { conversations: [], format: 'unknown', error: 'Invalid JSON' }
  }
}
