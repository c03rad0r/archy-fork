import { mkdirSync, writeFileSync, existsSync, readFileSync } from 'fs'
import { resolve } from 'path'
import { allTestConversations } from './fixtures/test-chats'

const CHATS_PATH = resolve(process.cwd(), '.dev', 'chats.json')

export default async function globalSetup() {
  const dir = resolve(process.cwd(), '.dev')
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })

  // Backup existing chats if present (for local dev)
  let backup: string | null = null
  if (existsSync(CHATS_PATH)) {
    backup = readFileSync(CHATS_PATH, 'utf-8')
  }

  const payload = {
    conversations: allTestConversations,
    activeConversationId: 'e2e-films',
  }
  writeFileSync(CHATS_PATH, JSON.stringify(payload, null, 2), 'utf-8')

  // Store backup path for teardown (we pass via env since globalSetup/Teardown don't share scope easily)
  if (backup) {
    process.env.AIUI_E2E_CHATS_BACKUP = backup
  }
}
