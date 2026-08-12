import { describe, expect, it } from 'vitest'
import { resolveAppCredentials } from '../appCredentials'

describe('resolveAppCredentials', () => {
  it('uses backend credentials when they are available', () => {
    expect(resolveAppCredentials('filebrowser', {
      title: 'Backend credentials',
      credentials: [{ label: 'Password', value: 'secret' }],
    })?.credentials[0]?.value).toBe('secret')
  })

  it('falls back to File Browser default credentials when backend data is not available', () => {
    const result = resolveAppCredentials('filebrowser', { credentials: [] })

    expect(result?.title).toBe('File Browser credentials')
    expect(result?.credentials).toEqual([
      { label: 'Username', value: 'admin' },
      { label: 'Password', value: 'admin', sensitive: true },
    ])
  })

  it('falls back to PhotoPrism manifest credentials when backend data is not available', () => {
    const result = resolveAppCredentials('photoprism', { credentials: [] })

    expect(result?.title).toBe('PhotoPrism credentials')
    expect(result?.credentials).toEqual([
      { label: 'Username', value: 'admin' },
      { label: 'Password', value: 'archipelago', sensitive: true },
    ])
  })

  it('does not invent credentials for unknown apps', () => {
    expect(resolveAppCredentials('unknown', { credentials: [] })).toBeNull()
  })
})
