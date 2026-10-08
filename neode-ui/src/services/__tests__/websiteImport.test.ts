import { beforeEach, describe, expect, it } from 'vitest'
import { pendingWebsiteHtml, prepareWebsiteImport } from '../websiteImport'
beforeEach(() => { pendingWebsiteHtml.value = null })
describe('AIUI website handoff', () => {
  it('holds HTML only in memory for explicit import', () => {
    expect(prepareWebsiteImport('<h1>My page</h1>')).toBe(true)
    expect(pendingWebsiteHtml.value).toBe('<h1>My page</h1>')
  })
  it('rejects invalid or oversized content without destroying a pending draft', () => {
    prepareWebsiteImport('existing')
    for (const bad of [null, {}, '', '\0', 'a'.repeat(512 * 1024 + 1)]) expect(prepareWebsiteImport(bad)).toBe(false)
    expect(pendingWebsiteHtml.value).toBe('existing')
  })
})
