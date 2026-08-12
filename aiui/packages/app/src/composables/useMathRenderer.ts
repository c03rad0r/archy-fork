/**
 * Lazy KaTeX math rendering for markdown content.
 * Detects $...$ (inline) and $$...$$ (block) LaTeX.
 * Falls back to raw <code> on invalid LaTeX.
 */

let katexModule: typeof import('katex') | null = null
let katexLoading: Promise<typeof import('katex')> | null = null

async function loadKaTeX() {
  if (katexModule) return katexModule
  if (katexLoading) return katexLoading
  katexLoading = import('katex').then((m) => {
    katexModule = m
    // Inject KaTeX CSS
    if (!document.getElementById('katex-css')) {
      const link = document.createElement('link')
      link.id = 'katex-css'
      link.rel = 'stylesheet'
      link.href = new URL('katex/dist/katex.min.css', import.meta.url).toString()
      document.head.appendChild(link)
    }
    return m
  })
  return katexLoading
}

const BLOCK_RE = /\$\$([\s\S]+?)\$\$/g
const INLINE_RE = /\$([^\n$]+?)\$/g

function renderKaTeX(latex: string, displayMode: boolean): string {
  if (!katexModule) return `<code>${escapeForHtml(latex)}</code>`
  try {
    return katexModule.default.renderToString(latex, {
      displayMode,
      throwOnError: false,
      output: 'html',
    })
  } catch {
    return `<code>${escapeForHtml(latex)}</code>`
  }
}

function escapeForHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function hasMath(text: string): boolean {
  return /\$\$[\s\S]+?\$\$/.test(text) || /\$[^\n$]+?\$/.test(text)
}

export async function renderMathInHtml(html: string): Promise<string> {
  await loadKaTeX()

  // Block math first ($$...$$)
  let result = html.replace(BLOCK_RE, (_, latex) => {
    return `<div class="katex-block my-4 text-center overflow-x-auto">${renderKaTeX(latex.trim(), true)}</div>`
  })

  // Inline math ($...$)
  result = result.replace(INLINE_RE, (_, latex) => {
    return renderKaTeX(latex.trim(), false)
  })

  return result
}
