import { afterEach, describe, expect, it, vi } from 'vitest'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
import { rpcClient } from '@/api/rpc-client'
import { installPublishingApp } from '../installPublishingApp'
afterEach(() => { vi.unstubAllGlobals(); vi.clearAllMocks() })
describe('Setup catalogue installation', () => {
  it('supplies the signed build tag and version required by package.install', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ apps: { 'public-web-router': { version: '0.1.0', manifest: { app: { id: 'public-web-router', container: { build: { tag: 'localhost/archipelago-public-web-router:0.1.0' } } } } } } }) }))
    await installPublishingApp('public-web-router')
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'package.install', params: { id: 'public-web-router', dockerImage: 'localhost/archipelago-public-web-router:0.1.0', version: '0.1.0' }, timeout: 600000, maxRetries: 0 })
  })
  it('does not install from an unavailable or mismatched catalogue', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: false }))
    await expect(installPublishingApp('blossom')).rejects.toThrow('unavailable')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue({ ok: true, json: async () => ({ apps: { blossom: { version: '1', image: 'localhost/test:1', manifest: { app: { id: 'other' } } } } }) }))
    await expect(installPublishingApp('blossom')).rejects.toThrow('not available')
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
})
