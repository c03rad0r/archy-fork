/**
 * Generate .dev/chats.json from the seed prompt index.
 * Run: pnpm -C packages/app exec tsx scripts/generate-seed-chats.ts
 */
import { seedPromptsToConversation } from '../src/__tests__/fixtures/seedPrompts'
import { writeFileSync, mkdirSync, existsSync } from 'fs'
import { resolve, dirname } from 'path'

const outPath = resolve(dirname(new URL(import.meta.url).pathname), '../.dev/chats.json')
const dir = dirname(outPath)
if (!existsSync(dir)) mkdirSync(dir, { recursive: true })

const conv = seedPromptsToConversation()
const data = {
  conversations: { [conv.id]: conv },
  activeConversationId: conv.id,
}

writeFileSync(outPath, JSON.stringify(data, null, 2), 'utf-8')
console.log(`Wrote seed conversation (${conv.messages.length / 2} prompts) to ${outPath}`)
