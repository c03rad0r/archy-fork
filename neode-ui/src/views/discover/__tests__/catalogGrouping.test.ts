import { describe, expect, it } from 'vitest'
import { signedCatalogToApps } from '../curatedApps'
import { normalizeStoreApps } from '../../apps/serviceNames'

describe('one App Store card per product', () => {
  it('keeps Cuprate and NetBird components out and prefers canonical BTCPay metadata', () => {
    const entries = {
      btcpay: { version: '2.4.3' },
      'btcpay-server': { version: '2.4.3', manifest: { app: { name: 'BTCPay Server', category: 'commerce', metadata: { icon: '/btcpay.svg' } } } },
      cuprate: { version: '0.1' }, 'cuprate-ui': { version: '1.7' },
      netbird: { version: '2.38' }, 'netbird-dashboard': { version: '2.38' }, 'netbird-server': { version: '0.71' },
    }
    for (const apps of [entries, Object.fromEntries(Object.entries(entries).reverse())]) {
      const result = signedCatalogToApps({ apps })
      expect(result.map(x => x.id).sort()).toEqual(['btcpay-server', 'cuprate', 'netbird'])
      expect(result.find(x => x.id === 'btcpay-server')).toMatchObject({ title: 'BTCPay Server', category: 'commerce', icon: '/btcpay.svg', version: '2.4.3' })
    }
  })
  it('normalizes persisted/community entries repeatedly without hiding independent services', () => {
    const stale = ['cuprate-ui', 'netbird-server', 'netbird-dashboard', 'btcpay', 'btcpay-server', 'angor-indexer', 'angor-relay', 'electrumx'].map(id => ({ id }))
    const result = normalizeStoreApps(stale)
    expect(result.map(x => x.id)).toEqual(['btcpay-server', 'angor-indexer', 'angor-relay', 'electrumx'])
    expect(normalizeStoreApps(result)).toEqual(result)
    expect(normalizeStoreApps([{ id: 'btcpay' }])).toEqual([{ id: 'btcpay-server' }])
  })
})
