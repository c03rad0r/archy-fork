import { writeFileSync } from 'fs'
import { resolve } from 'path'

const CHATS_PATH = resolve(process.cwd(), '.dev', 'chats.json')

export default async function globalTeardown() {
  const backup = process.env.AIUI_E2E_CHATS_BACKUP
  if (backup) {
    writeFileSync(CHATS_PATH, backup, 'utf-8')
  }
}
