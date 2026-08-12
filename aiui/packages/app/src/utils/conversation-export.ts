import type { Conversation } from '@aiui/core/types/message'

export type ExportFormat = 'markdown' | 'json' | 'text'

function formatTimestamp(ts: number): string {
  return new Date(ts).toLocaleString()
}

export function exportAsMarkdown(conv: Conversation): string {
  const lines = [`# ${conv.title}\n`, `_Exported ${formatTimestamp(Date.now())}_\n`]
  for (const msg of conv.messages) {
    const role = msg.role === 'user' ? '**You**' : '**Assistant**'
    const time = formatTimestamp(msg.timestamp)
    lines.push(`### ${role} — ${time}\n`)
    lines.push(msg.content + '\n')
  }
  return lines.join('\n')
}

export function exportAsJSON(conv: Conversation): string {
  return JSON.stringify(conv, null, 2)
}

export function exportAsText(conv: Conversation): string {
  const lines = [conv.title, '='.repeat(conv.title.length), '']
  for (const msg of conv.messages) {
    const role = msg.role === 'user' ? 'You' : 'Assistant'
    lines.push(`[${role}] ${formatTimestamp(msg.timestamp)}`)
    lines.push(msg.content)
    lines.push('')
  }
  return lines.join('\n')
}

function getExtension(format: ExportFormat): string {
  switch (format) {
    case 'markdown': return '.md'
    case 'json': return '.json'
    case 'text': return '.txt'
  }
}

function getMimeType(format: ExportFormat): string {
  switch (format) {
    case 'markdown': return 'text/markdown'
    case 'json': return 'application/json'
    case 'text': return 'text/plain'
  }
}

export async function downloadConversation(conv: Conversation, format: ExportFormat): Promise<void> {
  let content: string
  switch (format) {
    case 'markdown': content = exportAsMarkdown(conv); break
    case 'json': content = exportAsJSON(conv); break
    case 'text': content = exportAsText(conv); break
  }

  const filename = `${conv.title.replace(/[^a-zA-Z0-9 ]/g, '').trim().replace(/\s+/g, '-').toLowerCase()}${getExtension(format)}`
  const blob = new Blob([content], { type: getMimeType(format) })

  // Try File System Access API first
  if ('showSaveFilePicker' in window) {
    try {
      const handle = await (window as unknown as { showSaveFilePicker: (opts: unknown) => Promise<FileSystemFileHandle> }).showSaveFilePicker({
        suggestedName: filename,
        types: [{
          description: format === 'markdown' ? 'Markdown' : format === 'json' ? 'JSON' : 'Text',
          accept: { [getMimeType(format)]: [getExtension(format)] },
        }],
      })
      const writable = await handle.createWritable()
      await writable.write(blob)
      await writable.close()
      return
    } catch {
      // User cancelled or API not supported — fall through to download
    }
  }

  // Fallback: <a download>
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}
