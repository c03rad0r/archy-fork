import { shallowRef } from 'vue'

// In-memory handoff only. Receiving model content never writes files, opens a
// route or publishes. The trusted setup screen requires an explicit import.
export const pendingWebsiteHtml = shallowRef<string | null>(null)
export function prepareWebsiteImport(html: unknown): boolean {
  if (typeof html !== 'string' || !html.trim() || new TextEncoder().encode(html).length > 512 * 1024 || html.includes('\0')) return false
  pendingWebsiteHtml.value = html
  return true
}
