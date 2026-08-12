import type { Plugin, Connect } from 'vite'
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs'
import { resolve, dirname } from 'path'
import { validateDevAuth, checkRateLimit } from './server/dev-auth'

const CHATS_DIR = '.dev'
const CHATS_FILE = 'chats.json'
const MAX_BODY_SIZE = 5 * 1024 * 1024 // 5MB

function getChatsPath(root: string): string {
  const dir = resolve(root, CHATS_DIR)
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  return resolve(dir, CHATS_FILE)
}

function readChats(filePath: string): string {
  if (!existsSync(filePath)) return '{}'
  try {
    return readFileSync(filePath, 'utf-8')
  } catch {
    return '{}'
  }
}

function writeChats(filePath: string, data: string): void {
  const dir = dirname(filePath)
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  writeFileSync(filePath, data, 'utf-8')
}

export function devChatsPlugin(): Plugin {
  let chatsPath = ''

  return {
    name: 'aiui-dev-chats',
    apply: 'serve',
    configResolved(config) {
      chatsPath = getChatsPath(config.root)
    },
    configureServer(server) {
      server.middlewares.use('/api/dev-chats', (req: Connect.IncomingMessage, res: any, next: () => void) => {
        if (!validateDevAuth(req, res)) return
        if (!checkRateLimit(req, res, req.method === 'PUT')) return
        if (req.method === 'GET') {
          const data = readChats(chatsPath)
          res.setHeader('Content-Type', 'application/json')
          res.end(data)
          return
        }

        if (req.method === 'PUT') {
          let body = ''
          let exceeded = false
          req.on('data', (chunk: Buffer) => {
            if (exceeded) return
            body += chunk.toString()
            if (body.length > MAX_BODY_SIZE) {
              exceeded = true
              res.writeHead(413, { 'Content-Type': 'application/json' })
              res.end(JSON.stringify({ error: 'Payload too large (max 5MB)' }))
            }
          })
          req.on('end', () => {
            if (exceeded) return
            try {
              JSON.parse(body)
              writeChats(chatsPath, body)
              res.setHeader('Content-Type', 'application/json')
              res.end(JSON.stringify({ ok: true }))
            } catch (e) {
              res.writeHead(400, { 'Content-Type': 'application/json' })
              res.end(JSON.stringify({ error: 'Invalid JSON' }))
            }
          })
          return
        }

        next()
      })
    },
  }
}
