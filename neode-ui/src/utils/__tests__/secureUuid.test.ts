import { afterEach, describe, expect, it, vi } from 'vitest'
import { webcrypto } from 'node:crypto'
import { secureUuid } from '../secureUuid'

afterEach(() => vi.unstubAllGlobals())
describe('secure UUIDs across dashboard origins', () => {
  it('uses the native secure-context API with its receiver', () => {
    const source = { randomUUID() { expect(this).toBe(source); return 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa' }, getRandomValues: vi.fn() }
    vi.stubGlobal('crypto', source)
    expect(secureUuid()).toBe('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa')
    expect(source.getRandomValues).not.toHaveBeenCalled()
  })
  it('creates distinct UUIDs on LAN HTTP using only getRandomValues', () => {
    vi.stubGlobal('crypto', { getRandomValues: webcrypto.getRandomValues.bind(webcrypto) })
    const ids = Array.from({ length: 32 }, secureUuid)
    expect(new Set(ids).size).toBe(ids.length)
    for (const id of ids) expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/)
  })
  it('sets the UUID version and variant without losing other random bits', () => {
    vi.stubGlobal('crypto', { getRandomValues: (bytes: Uint8Array) => bytes.fill(255) })
    expect(secureUuid()).toBe('ffffffff-ffff-4fff-bfff-ffffffffffff')
  })
  it.each([undefined, {}])('refuses when secure randomness is unavailable (%s)', source => {
    vi.stubGlobal('crypto', source)
    expect(() => secureUuid()).toThrow('Secure randomness is unavailable.')
  })
})
