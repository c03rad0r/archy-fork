/**
 * Extract runnable code blocks (HTML, JS, CSS) from markdown text.
 */

export interface CodeBlock {
  language: string
  code: string
}

const RUNNABLE_LANGS = new Set(['html', 'javascript', 'js', 'css'])

const FENCED_CODE_RE = /```(html|javascript|js|css)\n([\s\S]*?)```/gi

export function extractRunnableCodeBlocks(text: string): CodeBlock[] {
  const results: CodeBlock[] = []
  let m: RegExpExecArray | null
  const re = new RegExp(FENCED_CODE_RE.source, FENCED_CODE_RE.flags)

  while ((m = re.exec(text)) !== null) {
    const language = m[1].toLowerCase()
    const code = m[2].trim()
    if (RUNNABLE_LANGS.has(language) && code.length > 0) {
      results.push({ language, code })
    }
  }

  return results
}

export function hasRunnableCode(text: string): boolean {
  return FENCED_CODE_RE.test(text)
}
