import type { Plugin } from 'vite'
import type { Connect } from 'vite'
import { readdirSync, statSync, readFileSync, existsSync, mkdirSync } from 'fs'
import { join, resolve, relative, basename } from 'path'
import { validateDevAuth, checkRateLimit } from './server/dev-auth'

const PROJECTS_ROOT = '/Users/dorian/Projects'

const IGNORED = new Set([
  'node_modules', '.git', 'dist', 'build', '.next', '.nuxt', '.output',
  '.cache', '.turbo', '.vercel', '.netlify', '__pycache__', 'target',
  '.DS_Store', 'coverage', '.vite', '.angular',
])

const MAX_FILE_SIZE = 1_000_000 // 1MB
const MAX_TREE_ENTRIES = 500
const MAX_DEPTH = 4

/** Validate path is within PROJECTS_ROOT to prevent directory traversal */
function isPathSafe(p: string): boolean {
  const resolved = resolve(p)
  return resolved.startsWith(PROJECTS_ROOT)
}

function parseUrl(req: Connect.IncomingMessage): URL {
  return new URL(req.url ?? '', `http://${req.headers?.host ?? 'localhost'}`)
}

/** GET /api/fs/list — list projects */
function handleList(_req: Connect.IncomingMessage, res: any) {
  try {
    const entries = readdirSync(PROJECTS_ROOT, { withFileTypes: true })
    const projects = entries
      .filter(e => e.isDirectory() && !e.name.startsWith('.'))
      .map(e => {
        const fullPath = join(PROJECTS_ROOT, e.name)
        const isGit = existsSync(join(fullPath, '.git'))
        let language = 'Unknown'

        // Detect project language from config files
        try {
          const files = readdirSync(fullPath).map(f => f)
          if (files.includes('package.json')) language = 'TypeScript/JavaScript'
          else if (files.includes('Cargo.toml')) language = 'Rust'
          else if (files.includes('go.mod')) language = 'Go'
          else if (files.includes('requirements.txt') || files.includes('pyproject.toml')) language = 'Python'
          else if (files.includes('pom.xml') || files.includes('build.gradle')) language = 'Java'
          else if (files.includes('Package.swift')) language = 'Swift'
          else if (files.includes('Gemfile')) language = 'Ruby'
          else if (files.includes('composer.json')) language = 'PHP'
          else if (files.some(f => f.endsWith('.csproj') || f.endsWith('.sln'))) language = 'C#'
        } catch { /* ignore read errors */ }

        return { name: e.name, path: fullPath, isGit, language }
      })
      .sort((a, b) => a.name.localeCompare(b.name))

    res.setHeader('Content-Type', 'application/json')
    res.end(JSON.stringify({ projects }))
  } catch (err) {
    res.writeHead(500, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: String(err) }))
  }
}

/** GET /api/fs/tree — recursive file tree */
function handleTree(req: Connect.IncomingMessage, res: any) {
  const url = parseUrl(req)
  const dirPath = url.searchParams.get('path')
  if (!dirPath || !isPathSafe(dirPath)) {
    res.writeHead(400, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Invalid or missing path' }))
    return
  }

  let entryCount = 0

  function walk(dir: string, depth: number): any[] {
    if (depth > MAX_DEPTH || entryCount >= MAX_TREE_ENTRIES) return []
    try {
      const entries = readdirSync(dir, { withFileTypes: true })
        .filter(e => !IGNORED.has(e.name) && (!e.name.startsWith('.') || e.name === '.claude'))
        .sort((a, b) => {
          // Directories first, then alphabetical
          if (a.isDirectory() !== b.isDirectory()) return a.isDirectory() ? -1 : 1
          return a.name.localeCompare(b.name)
        })

      const result: any[] = []
      for (const entry of entries) {
        if (entryCount >= MAX_TREE_ENTRIES) break
        entryCount++
        const fullPath = join(dir, entry.name)
        const relPath = relative(dirPath, fullPath)
        if (entry.isDirectory()) {
          result.push({
            name: entry.name,
            path: relPath,
            isDirectory: true,
            children: walk(fullPath, depth + 1),
          })
        } else {
          result.push({
            name: entry.name,
            path: relPath,
            isDirectory: false,
          })
        }
      }
      return result
    } catch {
      return []
    }
  }

  try {
    const files = walk(dirPath, 0)
    res.setHeader('Content-Type', 'application/json')
    res.end(JSON.stringify({ files }))
  } catch (err) {
    res.writeHead(500, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: String(err) }))
  }
}

/** Patterns that indicate sensitive files — block reads */
const SENSITIVE_PATTERNS = [
  /^\.env/i,
  /^\.git$/,
  /^\.git\//,
  /credentials/i,
  /secret/i,
  /\.pem$/i,
  /\.key$/i,
  /id_rsa/i,
  /id_ed25519/i,
]

function isSensitivePath(filePath: string): boolean {
  const resolved = resolve(filePath)
  const relToRoot = relative(PROJECTS_ROOT, resolved)
  const segments = relToRoot.split('/')
  return segments.some(seg => SENSITIVE_PATTERNS.some(p => p.test(seg)))
}

/** GET /api/fs/read — read file content */
function handleRead(req: Connect.IncomingMessage, res: any) {
  const url = parseUrl(req)
  const filePath = url.searchParams.get('path')
  if (!filePath || !isPathSafe(filePath)) {
    res.writeHead(400, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Invalid or missing path' }))
    return
  }

  if (isSensitivePath(filePath)) {
    res.writeHead(403, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Access denied: sensitive file' }))
    return
  }

  try {
    const stat = statSync(filePath)
    if (stat.isDirectory()) {
      res.writeHead(400, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Path is a directory' }))
      return
    }
    if (stat.size > MAX_FILE_SIZE) {
      res.writeHead(413, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'File too large', size: stat.size }))
      return
    }
    const content = readFileSync(filePath, 'utf-8')
    res.setHeader('Content-Type', 'application/json')
    res.end(JSON.stringify({ content, size: stat.size }))
  } catch (err: any) {
    const code = err?.code === 'ENOENT' ? 404 : 500
    res.writeHead(code, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: String(err) }))
  }
}

/** POST /api/fs/mkdir — create a directory */
function handleMkdir(req: Connect.IncomingMessage, res: any) {
  let body = ''
  req.on('data', (chunk: Buffer) => { body += chunk.toString() })
  req.on('end', () => {
    try {
      const { path: dirPath } = JSON.parse(body)
      if (!dirPath || !isPathSafe(dirPath)) {
        res.writeHead(400, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'Invalid or missing path' }))
        return
      }
      if (existsSync(dirPath)) {
        res.writeHead(409, { 'Content-Type': 'application/json' })
        res.end(JSON.stringify({ error: 'Directory already exists' }))
        return
      }
      mkdirSync(dirPath, { recursive: true })
      res.setHeader('Content-Type', 'application/json')
      res.end(JSON.stringify({ ok: true, path: dirPath }))
    } catch (err) {
      res.writeHead(500, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: String(err) }))
    }
  })
}

export function fsPlugin(): Plugin {
  return {
    name: 'aiui-fs',
    configureServer(server) {
      server.middlewares.use('/api/fs/list', (req, res, next) => {
        if (req.method !== 'GET') return next()
        if (!validateDevAuth(req, res)) return
        if (!checkRateLimit(req, res)) return
        handleList(req, res)
      })
      server.middlewares.use('/api/fs/tree', (req, res, next) => {
        if (req.method !== 'GET') return next()
        if (!validateDevAuth(req, res)) return
        if (!checkRateLimit(req, res)) return
        handleTree(req, res)
      })
      server.middlewares.use('/api/fs/read', (req, res, next) => {
        if (req.method !== 'GET') return next()
        if (!validateDevAuth(req, res)) return
        if (!checkRateLimit(req, res)) return
        handleRead(req, res)
      })
      server.middlewares.use('/api/fs/mkdir', (req, res, next) => {
        if (req.method !== 'POST') return next()
        if (!validateDevAuth(req, res)) return
        if (!checkRateLimit(req, res, true)) return
        handleMkdir(req, res)
      })
    },
  }
}
