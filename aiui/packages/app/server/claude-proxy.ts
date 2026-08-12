import { spawn, execSync } from 'child_process'
import { createServer } from 'http'
import { readFileSync, existsSync } from 'fs'
import { resolve, dirname } from 'path'
import { fileURLToPath } from 'url'
import { validateDevAuth, handleCorsOptions, checkRateLimit } from './dev-auth.js'

const __dirname = dirname(fileURLToPath(import.meta.url))

// Load .env.local from workspace root (monorepo) or cwd
function loadEnv() {
  for (const base of [resolve(__dirname, '../../..'), process.cwd()]) {
    const path = resolve(base, '.env.local')
    if (existsSync(path)) {
      try {
        const buf = readFileSync(path, 'utf8')
        for (const line of buf.split('\n')) {
          const m = line.match(/^([^#=]+)=(.*)$/)
          if (m) {
            const key = m[1].trim()
            const val = m[2].trim().replace(/^["']|["']$/g, '')
            if (!process.env[key]) process.env[key] = val
          }
        }
        break
      } catch {
        /* ignore */
      }
    }
  }
}
loadEnv()

const PORT = 3141

/**
 * Resolve the `claude` CLI binary. The old hardcoded `~/.local/bin/claude`
 * broke on any machine/install where the CLI lives elsewhere (e.g. an nvm
 * Node install's own bin dir) — ENOENT on spawn with no obvious fix short of
 * a manual symlink. Prefer a real PATH lookup, matching how a user would
 * actually run `claude` themselves; fall back to the historical path for
 * anyone relying on it, then to the bare command name so spawn() still gets
 * a chance to resolve it via PATH at process-start time even if neither
 * check above found it (e.g. PATH changes after this proxy boots).
 */
function resolveClaudeBin(): string {
  if (process.env.CLAUDE_BIN && existsSync(process.env.CLAUDE_BIN)) {
    return process.env.CLAUDE_BIN
  }
  try {
    const found = execSync('command -v claude', { encoding: 'utf8' }).trim()
    if (found) return found
  } catch {
    /* not resolvable via PATH right now — fall through */
  }
  const legacy = resolve(process.env.HOME ?? '', '.local/bin/claude')
  if (existsSync(legacy)) return legacy
  return 'claude'
}

const CLAUDE_BIN = resolveClaudeBin()
const APP_URL = process.env.APP_URL ?? 'http://localhost:5173'

/** API key (sk-ant-api03-...) or OAuth token (sk-ant-oat...) from Max subscription */
function getAnthropicCredential(): string | undefined {
  const fromEnv = process.env.ANTHROPIC_API_KEY
    ?? process.env.VITE_ANTHROPIC_API_KEY
    ?? process.env.ANTHROPIC_TOKEN
    ?? process.env.VITE_ANTHROPIC_TOKEN
  if (fromEnv) return fromEnv
  const home = process.env.HOME ?? ''
  const settingsPath = resolve(home, '.claude/settings.json')
  if (home && existsSync(settingsPath)) {
    try {
      const json = JSON.parse(readFileSync(settingsPath, 'utf8'))
      const env = json?.env
      if (env && typeof env === 'object') {
        const t = env.ANTHROPIC_TOKEN ?? env.VITE_ANTHROPIC_TOKEN ?? env.ANTHROPIC_API_KEY ?? env.VITE_ANTHROPIC_API_KEY
        if (typeof t === 'string') return t
      }
    } catch { /* ignore */ }
  }
  // macOS keychain: Claude Code stores OAuth credentials here
  if (process.platform === 'darwin') {
    try {
      const raw = execSync(
        'security find-generic-password -s "Claude Code-credentials" -w 2>/dev/null',
        { encoding: 'utf8', timeout: 3000 },
      ).trim()
      const creds = JSON.parse(raw)
      const oauthToken = creds?.claudeAiOauth?.accessToken
      if (typeof oauthToken === 'string' && oauthToken.startsWith('sk-ant-')) {
        console.log('[proxy] Found Claude OAuth token in macOS keychain')
        return oauthToken
      }
    } catch { /* keychain not available or no entry */ }
  }
  return undefined
}

const ANTHROPIC_CREDENTIAL = getAnthropicCredential()
const OPENROUTER_API_KEY = process.env.OPENROUTER_API_KEY ?? process.env.VITE_OPENROUTER_API_KEY ?? ''
const isOAuthToken = (s: string) => /^sk-ant-oat/.test(s)

const SEARCH_WEB_TOOL = {
  name: 'search_web',
  description: 'Search the web for current information. Use this when the user asks for news, recent events, facts you are unsure about, or any information that may have changed. Perform one search per distinct topic. Returns titles, URLs, and snippets.',
  input_schema: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'Search query (e.g. "Bitcoin price March 2025", "latest news AI regulation")',
      },
    },
    required: ['query'],
  },
}

function mapModelToApi(model: string): string {
  if (model?.includes('opus')) return 'claude-opus-4-20250514'
  if (model?.includes('haiku')) return 'claude-haiku-4-5-20251001'
  return 'claude-sonnet-4-20250514'
}

async function runSearchWeb(query: string): Promise<string> {
  const url = `${APP_URL.replace(/\/$/, '')}/api/web-search?${new URLSearchParams({ q: query })}`
  try {
    const res = await fetch(url, {
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(10000),
    })
    if (!res.ok) return `Search failed: ${res.status}`
    const data = (await res.json()) as { results?: { title?: string; url?: string; content?: string }[] }
    const results = data.results ?? []
    if (results.length === 0) return 'No results found.'
    return results
      .map((r, i) => `${i + 1}. [${r.title ?? 'Unknown'}](${r.url ?? ''})${r.content ? ` — ${r.content.slice(0, 150)}${r.content.length > 150 ? '…' : ''}` : ''}`)
      .join('\n')
  } catch (err) {
    return `Search error: ${err instanceof Error ? err.message : String(err)}`
  }
}

async function streamViaAnthropicApi(
  model: string,
  system: string | undefined,
  messages: { role: string; content: unknown }[],
  res: import('http').ServerResponse,
  useTools: boolean,
  credential: string,
  maxTokens?: number,
): Promise<void> {
  const apiModel = mapModelToApi(model)
  const apiMessages = messages.map((m) => ({
    role: m.role === 'assistant' ? 'assistant' : 'user',
    content: typeof m.content === 'string' ? m.content : m.content,
  }))

  let clientDisconnected = false
  res.on('close', () => { clientDisconnected = true })

  const sendDelta = (text: string) => {
    if (!clientDisconnected) {
      res.write(`data: ${JSON.stringify({ type: 'content_block_delta', delta: { type: 'text_delta', text } })}\n\n`)
    }
  }

  const sendError = (msg: string) => {
    if (!clientDisconnected) {
      res.write(`data: ${JSON.stringify({ type: 'error', error: { message: msg } })}\n\n`)
    }
  }

  const buildHeaders = (): Record<string, string> => {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'anthropic-version': '2023-06-01',
    }
    if (isOAuthToken(credential)) {
      headers['Authorization'] = `Bearer ${credential}`
      headers['anthropic-beta'] = 'oauth-2025-04-20'
    } else {
      headers['x-api-key'] = credential
    }
    return headers
  }

  // Tool use loop (non-streaming — needs to collect tool calls)
  if (useTools) {
    let turnMessages = [...apiMessages]
    const maxToolRounds = 5
    let rounds = 0

    while (rounds < maxToolRounds) {
      rounds++
      const body: Record<string, unknown> = {
        model: apiModel,
        max_tokens: maxTokens ?? 4096,
        system,
        messages: turnMessages,
        tools: [SEARCH_WEB_TOOL],
      }

      const apiRes = await fetch('https://api.anthropic.com/v1/messages', {
        method: 'POST',
        headers: buildHeaders(),
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(120000),
      })

      if (!apiRes.ok) {
        const errBody = await apiRes.text()
        sendError(`Anthropic API ${apiRes.status}: ${errBody.slice(0, 200)}`)
        break
      }

      const data = (await apiRes.json()) as {
        content?: { type: string; text?: string; id?: string; name?: string; input?: { query?: string } }[]
        stop_reason?: string
      }

      const content = data.content ?? []
      const toolUses = content.filter((b) => b.type === 'tool_use')
      const textBlocks = content.filter((b) => b.type === 'text')

      if (data.stop_reason === 'tool_use' && toolUses.length > 0) {
        const toolResults: { type: string; tool_use_id: string; content: string }[] = []
        for (const tu of toolUses) {
          if (tu.name === 'search_web' && tu.id && tu.input?.query) {
            console.log('[proxy] tool search_web:', tu.input.query)
            const result = await runSearchWeb(tu.input.query)
            toolResults.push({ type: 'tool_result', tool_use_id: tu.id, content: result })
          }
        }
        turnMessages = [
          ...turnMessages,
          { role: 'assistant' as const, content },
          { role: 'user' as const, content: toolResults },
        ]
        continue
      }

      for (const block of textBlocks) {
        if (block.text) sendDelta(block.text)
      }
      break
    }

    if (!clientDisconnected) {
      res.write('data: [DONE]\n\n')
      res.end()
    }
    return
  }

  // Streaming path (no tools)
  const body: Record<string, unknown> = {
    model: apiModel,
    max_tokens: maxTokens ?? 4096,
    stream: true,
    messages: apiMessages,
  }
  if (system) body.system = system

  const apiRes = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: buildHeaders(),
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(120000),
  })

  if (!apiRes.ok) {
    const errBody = await apiRes.text()
    sendError(`Anthropic API ${apiRes.status}: ${errBody.slice(0, 200)}`)
    if (!clientDisconnected) {
      res.write('data: [DONE]\n\n')
      res.end()
    }
    return
  }

  // Pipe SSE stream from Anthropic to client
  const reader = apiRes.body?.getReader()
  if (!reader) {
    sendError('No response body from API')
    if (!clientDisconnected) {
      res.write('data: [DONE]\n\n')
      res.end()
    }
    return
  }

  const decoder = new TextDecoder()
  try {
    while (true) {
      if (clientDisconnected) break
      const { done, value } = await reader.read()
      if (done) break
      const chunk = decoder.decode(value, { stream: true })
      res.write(chunk)
    }
  } catch (err) {
    if (!clientDisconnected) {
      sendError(`Stream error: ${err instanceof Error ? err.message : String(err)}`)
    }
  } finally {
    reader.cancel().catch(() => {})
    if (!clientDisconnected) {
      res.end()
    }
  }
}

async function streamOpenRouterProxy(
  reqBody: string,
  res: import('http').ServerResponse,
): Promise<void> {
  if (!OPENROUTER_API_KEY) {
    res.writeHead(500, { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': 'http://localhost:5173' })
    res.end(JSON.stringify({ error: 'OPENROUTER_API_KEY not configured on server' }))
    return
  }

  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Connection': 'keep-alive',
    'Access-Control-Allow-Origin': 'http://localhost:5173',
    'X-Accel-Buffering': 'no',
  })

  let clientDisconnected = false
  res.on('close', () => { clientDisconnected = true })

  try {
    const apiRes = await fetch('https://openrouter.ai/api/v1/chat/completions', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${OPENROUTER_API_KEY}`,
        'HTTP-Referer': APP_URL,
        'X-Title': 'AIUI',
      },
      body: reqBody,
      signal: AbortSignal.timeout(120000),
    })

    if (!apiRes.ok) {
      const errBody = await apiRes.text()
      if (!clientDisconnected) {
        res.write(`data: ${JSON.stringify({ error: `OpenRouter API ${apiRes.status}: ${errBody.slice(0, 200)}` })}\n\n`)
        res.write('data: [DONE]\n\n')
        res.end()
      }
      return
    }

    const reader = apiRes.body?.getReader()
    if (!reader) {
      if (!clientDisconnected) {
        res.write('data: [DONE]\n\n')
        res.end()
      }
      return
    }

    const decoder = new TextDecoder()
    while (true) {
      const { done, value } = await reader.read()
      if (done || clientDisconnected) break
      const chunk = decoder.decode(value, { stream: true })
      res.write(chunk)
    }

    if (!clientDisconnected) {
      res.end()
    }
  } catch (err) {
    console.error('[proxy] OpenRouter error:', err)
    if (!clientDisconnected) {
      res.write(`data: ${JSON.stringify({ error: `OpenRouter proxy error: ${err instanceof Error ? err.message : String(err)}` })}\n\n`)
      res.write('data: [DONE]\n\n')
      res.end()
    }
  }
}

const server = createServer((req, res) => {
  if (req.method === 'OPTIONS') {
    handleCorsOptions(res)
    return
  }

  if (req.method !== 'POST' || (req.url !== '/v1/messages' && req.url !== '/v1/openrouter')) {
    res.writeHead(404, { 'Content-Type': 'application/json' })
    res.end(JSON.stringify({ error: 'Not found' }))
    return
  }

  if (!validateDevAuth(req, res)) return
  if (!checkRateLimit(req, res, true)) return

  const MAX_BODY_SIZE = 1 * 1024 * 1024 // 1 MB
  let body = ''
  let aborted = false
  req.on('data', (chunk) => {
    body += chunk
    if (body.length > MAX_BODY_SIZE) {
      aborted = true
      res.writeHead(413, { 'Content-Type': 'application/json' })
      res.end(JSON.stringify({ error: 'Request body too large (max 1MB)' }))
      req.destroy()
    }
  })
  req.on('end', () => {
    if (aborted) return
    if (req.url === '/v1/openrouter') {
      console.log('[proxy] → OpenRouter proxy')
      streamOpenRouterProxy(body, res)
      return
    }
    try {
      const payload = JSON.parse(body)
      const { model, messages, system, webSearch, max_tokens } = payload

      // Use client-provided API key if present, otherwise fall back to server credential
      const clientKey = req.headers['x-api-key'] as string | undefined
      const credential = clientKey || ANTHROPIC_CREDENTIAL

      if (credential) {
        // Direct API — fast streaming
        const useTools = webSearch === true
        const apiModel = mapModelToApi(model)
        console.log(`[proxy] → Anthropic API ${apiModel}${useTools ? ' [tools]' : ' [stream]'}`)

        res.writeHead(200, {
          'Content-Type': 'text/event-stream',
          'Cache-Control': 'no-cache',
          'Connection': 'keep-alive',
          'Access-Control-Allow-Origin': '*',
          'X-Accel-Buffering': 'no',
        })
        streamViaAnthropicApi(model, system, messages ?? [], res, useTools, credential, max_tokens)
        return
      }

      // CLI fallback — no API credential available
      const modelFlag = model?.includes('opus') ? 'opus'
        : model?.includes('haiku') ? 'haiku'
        : 'sonnet'

      const history = (messages ?? []) as { role: string; content: string }[]
      const userMessages = history.filter((m) => m.role === 'user')
      const lastUserMsg = userMessages[userMessages.length - 1]?.content ?? ''

      const contextParts: string[] = []
      if (system) contextParts.push(system)
      const prior = history.slice(0, -1)
      if (prior.length > 0) {
        contextParts.push(
          'Conversation so far:\n' +
          prior.map((m) => `${m.role}: ${m.content}`).join('\n')
        )
      }

      const systemPrompt = contextParts.length > 0 ? contextParts.join('\n\n') : undefined

      const args = ['-p', '--model', modelFlag]
      if (systemPrompt) args.push('--system-prompt', systemPrompt)
      if (webSearch === true) {
        args.push('--allowed-tools', 'WebSearch', 'WebFetch')
        args.push('--permission-mode', 'dontAsk')
      }
      args.push('--', lastUserMsg)

      console.log(`[proxy] → claude CLI --model ${modelFlag}${webSearch ? ' [WebSearch]' : ''} "${lastUserMsg.slice(0, 60)}..."`)

      const procEnv = { ...process.env, NO_COLOR: '1', TERM: 'dumb' }
      delete procEnv.CLAUDECODE
      delete procEnv.CLAUDE_CODE
      delete procEnv.ANTHROPIC_CLAUDE_CODE
      delete procEnv.CLAUDE_CODE_ENTRYPOINT
      if (webSearch === true) {
        delete procEnv.DISALLOWED_TOOLS
      }
      const proc = spawn(CLAUDE_BIN, args, {
        stdio: ['pipe', 'pipe', 'pipe'],
        env: procEnv,
        detached: false,
      })

      proc.stdin.end('')

      res.writeHead(200, {
        'Content-Type': 'text/event-stream',
        'Cache-Control': 'no-cache',
        'Connection': 'keep-alive',
        'Access-Control-Allow-Origin': '*',
        'X-Accel-Buffering': 'no',
      })

      let fullOutput = ''
      let clientDisconnected = false

      proc.stdout.on('data', (chunk: Buffer) => {
        const text = chunk.toString()
        fullOutput += text

        if (!clientDisconnected) {
          const sseData = {
            type: 'content_block_delta',
            delta: { type: 'text_delta', text },
          }
          res.write(`data: ${JSON.stringify(sseData)}\n\n`)
        }
      })

      proc.stderr.on('data', (chunk: Buffer) => {
        const msg = chunk.toString().trim()
        if (msg) console.error('[proxy] stderr:', msg)
      })

      proc.on('error', (err: NodeJS.ErrnoException) => {
        console.error('[proxy] spawn error:', err)
        if (!clientDisconnected) {
          const message = err.code === 'ENOENT'
            ? `Claude CLI not found (tried "${CLAUDE_BIN}"). Install it (npm i -g @anthropic-ai/claude-code), ` +
              `make sure it's on PATH, or set CLAUDE_BIN to its full path in .env.local. ` +
              `Alternatively, configure ANTHROPIC_API_KEY or ANTHROPIC_TOKEN in .env.local to skip the CLI entirely.`
            : `Spawn error: ${err.message}`
          const errData = {
            type: 'error',
            error: { message },
          }
          res.write(`data: ${JSON.stringify(errData)}\n\n`)
          res.write('data: [DONE]\n\n')
          res.end()
        }
      })

      proc.on('close', (code, signal) => {
        console.log(`[proxy] ← exit code=${code} signal=${signal} output=${fullOutput.length}b`)
        if (!clientDisconnected) {
          res.write('data: [DONE]\n\n')
          res.end()
        }
      })

      res.on('close', () => {
        clientDisconnected = true
        if (proc.exitCode === null && !proc.killed) {
          console.log('[proxy] Client disconnected, killing process')
          proc.kill('SIGTERM')
        }
      })

    } catch (err) {
      console.error('[proxy] Parse error:', err)
      res.writeHead(400, {
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': 'http://localhost:5173',
      })
      res.end(JSON.stringify({ error: String(err) }))
    }
  })
})

server.listen(PORT, () => {
  console.log(`\n  Claude proxy → http://localhost:${PORT}`)
  console.log(`  Binary: ${CLAUDE_BIN}`)
  if (ANTHROPIC_CREDENTIAL) {
    const mode = isOAuthToken(ANTHROPIC_CREDENTIAL) ? 'OAuth (Max)' : 'API key'
    console.log(`  Tool use (search_web): enabled (${mode})`)
  } else {
    console.log(`  Tool use: add ANTHROPIC_TOKEN (Max) or ANTHROPIC_API_KEY to .env.local`)
  }
  console.log(`  OpenRouter proxy: ${OPENROUTER_API_KEY ? 'enabled' : 'add OPENROUTER_API_KEY to .env.local'}\n`)
})
