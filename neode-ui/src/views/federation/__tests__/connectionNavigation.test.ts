import { describe, expect, it } from 'vitest'
import { cloudSelection, connectionReturn } from '../connectionNavigation'

describe('connection return destinations', () => {
  it('preserves Cloud tab/category and Fleet destinations without arbitrary redirects', () => {
    expect(connectionReturn({ from: 'cloud', tab: 'peers', category: 'music' })).toEqual({ label: 'Cloud', to: { name: 'cloud', query: { tab: 'peers', category: 'music' } } })
    expect(connectionReturn({ from: 'fleet' })).toEqual({ label: 'Fleet', to: { name: 'fleet' } })
    for (const from of ['https://evil.invalid', '//evil.invalid', '/logout', ['fleet']]) expect(connectionReturn({ from }).to).toBe('/dashboard/web5')
    expect(cloudSelection({ tab: ['mine'], category: 'anything' })).toEqual({ tab: 'folders', category: 'all' })
  })
  it('returns only to a bounded peer route parameter', () => {
    expect(connectionReturn({ from: 'peer-files', peer: 'fixture.onion' }).to).toEqual({ name: 'peer-files', params: { peerId: 'fixture.onion' } })
    for (const peer of ['', '../wallet', 'a?token=b', 'a/b', 'a\\b', 'a\nb', 'a'.repeat(257)]) expect(connectionReturn({ from: 'peer-files', peer }).to).toBe('/dashboard/web5')
  })
})
