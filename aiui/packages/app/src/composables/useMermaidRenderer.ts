/**
 * Lazy Mermaid diagram rendering for ```mermaid code blocks.
 * Dark theme matching glass design, cached renders.
 */

let mermaidModule: typeof import('mermaid') | null = null
let mermaidLoading: Promise<typeof import('mermaid')> | null = null
let mermaidInitialized = false

async function loadMermaid() {
  if (mermaidModule) return mermaidModule
  if (mermaidLoading) return mermaidLoading
  mermaidLoading = import('mermaid').then((m) => {
    mermaidModule = m
    return m
  })
  return mermaidLoading
}

function initMermaid() {
  if (mermaidInitialized || !mermaidModule) return
  mermaidModule.default.initialize({
    startOnLoad: false,
    theme: 'dark',
    themeVariables: {
      primaryColor: '#F7931A',
      primaryTextColor: '#fff',
      primaryBorderColor: '#F7931A',
      lineColor: '#666',
      secondaryColor: '#1a1a1a',
      tertiaryColor: '#111',
      background: '#0a0a0a',
      mainBkg: '#1a1a1a',
      nodeBorder: '#444',
      clusterBkg: '#111',
      clusterBorder: '#333',
      titleColor: '#ddd',
      edgeLabelBackground: '#1a1a1a',
    },
    fontFamily: 'Inter, system-ui, sans-serif',
    fontSize: 13,
  })
  mermaidInitialized = true
}

// Cache rendered diagrams
const renderCache = new Map<string, string>()

export function hasMermaid(text: string): boolean {
  return /```mermaid/i.test(text)
}

let renderCounter = 0

export async function renderMermaidBlocks(html: string): Promise<string> {
  await loadMermaid()
  initMermaid()

  const mermaid = mermaidModule!.default

  // Find <pre><code class="language-mermaid">...</code></pre> blocks
  const PRE_RE = /<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>/gi
  const matches = [...html.matchAll(PRE_RE)]
  if (matches.length === 0) return html

  let result = html
  for (const match of matches) {
    const raw = match[1]
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .trim()

    const cacheKey = raw
    if (renderCache.has(cacheKey)) {
      result = result.replace(match[0], renderCache.get(cacheKey)!)
      continue
    }

    try {
      const id = `mermaid-${++renderCounter}`
      const { svg } = await mermaid.render(id, raw)
      const wrapped = `<div class="mermaid-diagram my-4 overflow-x-auto rounded-lg bg-white/5 border border-white/5 p-4">${svg}</div>`
      renderCache.set(cacheKey, wrapped)
      result = result.replace(match[0], wrapped)
    } catch {
      // Show error inline without crashing
      const errHtml = `<div class="my-4 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-xs text-red-400/70">Mermaid render error</div><pre><code>${match[1]}</code></pre>`
      result = result.replace(match[0], errHtml)
    }
  }

  return result
}
