import { describe, expect, it, vi } from 'vitest'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
import { PUBLISH_ROUTES, publishing, websitePreview } from '../publishing'
import { rpcClient } from '@/api/rpc-client'

describe('publishing trust boundaries', () => {
  it('places restrictive CSP before untrusted website content', () => {
    const hostile = '<script>fetch("/rpc")</script><meta http-equiv="Content-Security-Policy" content="default-src *">'
    const preview = websitePreview(hostile)
    expect(preview.indexOf("default-src 'none'")).toBeLessThan(preview.indexOf(hostile))
    expect(preview).toContain("form-action 'none'")
    expect(preview).not.toContain("script-src 'unsafe-inline'")
  })
  it('supports all four routes without Tailscale', () => {
    expect(PUBLISH_ROUTES.map(r => r.id)).toEqual(['fips', 'public-web', 'tor', 'nostr'])
  })
  it('does not retry ambiguous writes and carries the node version', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ state: { version: 8 }, project_id: null })
    await publishing.update(7, { action: 'connections', routes: ['fips', 'tor'] })
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'publishing.update', params: { version: 7, change: { action: 'connections', routes: ['fips', 'tor'] } }, maxRetries: 0 })
  })
})
