import { beforeEach, describe, expect, it, vi } from 'vitest'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('../publishing', () => ({ publishing: { status: vi.fn(), update: vi.fn() } }))
import { rpcClient } from '@/api/rpc-client'
import { publishing } from '../publishing'
import { namedNsiteUrl, prepareNsite, publishNsite, relayAddresses, retryNsite, requestNsiteDeletion, nsiteIdentities, storeLocalWebsite } from '../nsitePublishing'
import type { NsiteReceipt, SignedNsiteEvent } from '../nsitePublishing'
const identity = { id: 'profile', name: 'Me', nostr_pubkey: 'a'.repeat(64), is_node: false }
const event: SignedNsiteEvent = { id: 'b'.repeat(64), pubkey: identity.nostr_pubkey, kind: 35128, created_at: 1, tags: [['d', 'website123']], content: '', sig: 'c'.repeat(128) }
const receipt: NsiteReceipt = { identity_id: identity.id, server: 'https://blossom.example', event, accepted_relays: [], deletion_requested: false }
let accept = true
class Socket {
  onopen?: () => void
  onmessage?: (event: { data: string }) => void
  onerror?: () => void
  onclose?: () => void
  constructor() { queueMicrotask(() => this.onopen?.()) }
  send(raw: string) {
    const sent = JSON.parse(raw)[1]
    queueMicrotask(() => {
      this.onmessage?.({ data: JSON.stringify(['OK', 'unrelated-id', true]) })
      this.onmessage?.({ data: JSON.stringify(['OK', sent.id, accept]) })
    })
  }
  close() {}
}
const prepared = { html: '<h1>Hello 🏝</h1>', sha256: 'd'.repeat(64), server: receipt.server, identifier: 'website123', authorization: { kind: 24242, tags: [['expiration', String(Math.floor(Date.now() / 1000) + 300)]] }, manifest: { ...event } }
async function publishReviewed(html: string) {
  const p = await prepareNsite('project', 4, receipt.server, html)
  return publishNsite('project', 4, identity, ['wss://relay.example'], p)
}
beforeEach(() => {
  vi.clearAllMocks(); accept = true
  vi.stubGlobal('WebSocket', Socket)
  vi.mocked(publishing.status).mockResolvedValue({ state: { version: 4, projects: {} } } as never)
  vi.mocked(publishing.update).mockResolvedValue({} as never)
  vi.mocked(rpcClient.call).mockImplementation(async request => {
    if (request.method === 'publishing.nsite-prepare') return prepared as never
    if (request.method === 'identity.nostr-sign') return { ...event, ...(request.params as {event: object}).event } as never
    if (request.method === 'identity.list') return { identities: [identity, { ...identity, id: 'node', is_node: true }] } as never
    throw new Error('Unexpected RPC')
  })
  vi.stubGlobal('fetch', vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({ sha256: prepared.sha256, size: new TextEncoder().encode(prepared.html).length }), { status: 201 })).mockResolvedValueOnce(new Response(prepared.html)))
})
describe('named nsite publishing', () => {
  it('publishes local Blossom bytes through the node adapter and reads the public hash before announcing', async () => {
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async request => request.method === 'publishing.blossom-store' ? {} as never : original(request))
    vi.mocked(fetch).mockReset().mockResolvedValue(new Response(prepared.html))
    await publishNsite('project', 4, identity, ['wss://relay.example'], { ...prepared, local: true })
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'publishing.blossom-store', params: expect.objectContaining({ nsite: { html: prepared.html, server: prepared.server, acknowledge_public: true } }) }))
    expect(fetch).toHaveBeenCalledTimes(1)
    expect(fetch).toHaveBeenCalledWith(`${prepared.server}/${prepared.sha256}`, expect.objectContaining({ credentials: 'omit', redirect: 'error' }))
    expect(publishing.update).toHaveBeenCalledTimes(2)
  })

  it('stores locally through the authenticated node adapter without external uploads or broadcasts', async () => {
    const socket = vi.fn()
    vi.stubGlobal('WebSocket', socket)
    vi.mocked(rpcClient.call).mockImplementation(async request => {
      if (request.method === 'publishing.blossom-prepare') return { authorization: prepared.authorization } as never
      if (request.method === 'identity.nostr-sign') return { ...event, ...(request.params as {event: object}).event } as never
      if (request.method === 'publishing.blossom-store') return {} as never
      throw new Error('Unexpected RPC')
    })
    await storeLocalWebsite('project', 4, identity)
    expect(vi.mocked(rpcClient.call).mock.calls.map(([r]) => r.method)).toEqual(['publishing.blossom-prepare', 'identity.nostr-sign', 'publishing.blossom-store'])
    expect(fetch).not.toHaveBeenCalled()
    expect(socket).not.toHaveBeenCalled()
    expect(publishing.update).not.toHaveBeenCalled()
    await expect(storeLocalWebsite('project', 4, { ...identity, is_node: true })).rejects.toThrow('profile identity')
  })
  it('uses profile identities and rejects insecure relay URLs', async () => {
    expect(await nsiteIdentities()).toEqual([identity])
    expect(relayAddresses('wss://relay.example wss://relay.example')).toEqual(['wss://relay.example/'])
    for (const value of ['ws://relay.example', 'wss://user:secret@relay.example', 'wss://relay.example/#key']) expect(() => relayAddresses(value)).toThrow()
  })
  it('excludes legacy node identities before any local upload signing', async () => {
    const hidden = [
      { ...identity, id: ' Node-legacy ', is_node: false },
      { ...identity, name: '\uFEFFNode ', is_node: false },
      { ...identity, nostr_pubkey: 'invalid' },
    ]
    vi.mocked(rpcClient.call).mockResolvedValueOnce({ identities: [identity, ...hidden] } as never)
    expect(await nsiteIdentities()).toEqual([identity])
    vi.mocked(rpcClient.call).mockClear()
    for (const candidate of hidden) await expect(storeLocalWebsite('project', 4, candidate)).rejects.toThrow('profile identity')
    expect(rpcClient.call).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
  })
  it('preparation never signs, uploads or broadcasts', async () => {
    await prepareNsite('project', 4, receipt.server, '<h1>Private draft</h1>')
    expect(fetch).not.toHaveBeenCalled()
    expect(publishing.update).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.map(([r]) => r.method)).toEqual(['publishing.nsite-prepare'])
  })
  it('refuses stale reviews before signing or uploading', async () => {
    await expect(publishNsite('project', 3, identity, ['wss://relay.example'], prepared)).rejects.toThrow('changed after review')
    expect(fetch).not.toHaveBeenCalled()
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('uploads exact UTF-8 bytes, scopes signing to the chosen profile, and records delivery', async () => {
    const result = await publishReviewed( '<meta http-equiv="refresh" content="0;url=https://tracker.example"><script>bad()</script><h1>Draft</h1>')
    expect(result.accepted_relays).toEqual(['wss://relay.example'])
    const prep = vi.mocked(rpcClient.call).mock.calls[0]![0].params as { html: string }
    expect(prep.html).not.toContain('<script')
    expect(prep.html).not.toContain('http-equiv')
    expect(prep.html).toContain('<h1>Draft</h1>')
    expect(fetch).toHaveBeenNthCalledWith(1, `${receipt.server}/upload`, expect.objectContaining({ method: 'PUT', credentials: 'omit', redirect: 'error', body: prepared.html }))
    expect(publishing.update).toHaveBeenCalledTimes(2)
    expect(vi.mocked(publishing.update).mock.calls[0]![1]).toMatchObject({ receipt: { accepted_relays: [] } })
    expect(vi.mocked(publishing.update).mock.calls[1]![1]).toMatchObject({ receipt: { accepted_relays: ['wss://relay.example'] } })
    const signs = vi.mocked(rpcClient.call).mock.calls.filter(([r]) => r.method === 'identity.nostr-sign')
    expect(signs.every(([r]) => r.params?.id === identity.id)).toBe(true)
  })
  it('never pays or announces when storage requires payment', async () => {
    vi.mocked(fetch).mockReset().mockResolvedValue(new Response('', { status: 402 }))
    await expect(publishReviewed( '<h1>Draft</h1>')).rejects.toThrow('No payment was made')
    expect(publishing.update).not.toHaveBeenCalled()
  })
  it('rejects altered signer output before upload or relay delivery', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async request => {
      if (request.method === 'identity.nostr-sign') return { ...event, ...prepared.authorization, tags: [['t', 'upload']] } as never
      throw new Error('Unexpected RPC')
    })
    await expect(publishNsite('project', 4, identity, ['wss://relay.example'], prepared)).rejects.toThrow('changed the reviewed event')
    expect(fetch).not.toHaveBeenCalled()
    expect(publishing.update).not.toHaveBeenCalled()
  })
  it('bounds untrusted upload receipts before announcing', async () => {
    vi.mocked(fetch).mockReset().mockResolvedValue(new Response(' '.repeat(8193)))
    await expect(publishReviewed('<h1>Draft</h1>')).rejects.toThrow('size limit')
    expect(publishing.update).not.toHaveBeenCalled()
  })
  it('does not announce if the server changes uploaded bytes', async () => {
    vi.mocked(fetch).mockReset().mockResolvedValueOnce(new Response(JSON.stringify({ sha256: prepared.sha256, size: new TextEncoder().encode(prepared.html).length }))).mockResolvedValueOnce(new Response('different'))
    await expect(publishReviewed( '<h1>Draft</h1>')).rejects.toThrow('different website bytes')
    expect(publishing.update).not.toHaveBeenCalled()
  })
  it('retains a pending manifest on rejection and retries without signing or uploading', async () => {
    accept = false
    await expect(publishReviewed( '<h1>Draft</h1>')).rejects.toThrow('No relay accepted')
    vi.clearAllMocks(); accept = true
    const result = await retryNsite('project', receipt, ['wss://relay.example'])
    expect(result.accepted_relays).toHaveLength(1)
    expect(fetch).not.toHaveBeenCalled()
    expect(rpcClient.call).not.toHaveBeenCalled()
  })
  it('does not retry announcements after local file sharing is revoked', async () => {
    vi.mocked(publishing.status).mockResolvedValue({ state: { version: 4, projects: { project: { domain: { hostname: 'blossom.example' }, fips_publication: {} } } } } as never)
    const socket = vi.fn()
    vi.stubGlobal('WebSocket', socket)
    await expect(retryNsite('project', receipt, ['wss://relay.example'])).rejects.toThrow('no longer shared')
    expect(socket).not.toHaveBeenCalled()
    expect(fetch).not.toHaveBeenCalled()
    expect(publishing.update).not.toHaveBeenCalled()
  })
  it('requests deletion with the publishing identity without deleting shared blobs', async () => {
    await requestNsiteDeletion('project', receipt, identity, ['wss://relay.example'])
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ params: { id: identity.id, event: expect.objectContaining({ kind: 5, tags: expect.arrayContaining([['e', event.id], ['a', `35128:${event.pubkey}:website123`]]) }) } }))
    expect(fetch).not.toHaveBeenCalled()
    expect(publishing.update).toHaveBeenCalledWith(4, expect.objectContaining({ receipt: expect.objectContaining({ deletion_requested: true }) }))
  })
  it('builds a portable named-site gateway URL without overwriting the root site', () => {
    const url = new URL(namedNsiteUrl(receipt, 'https://gateway.example'))
    expect(url.hostname.split('.')[0]).toHaveLength(50 + 'website123'.length)
    expect(url.hostname).toContain('website123.gateway.example')
    expect(() => namedNsiteUrl(receipt, 'http://gateway.example')).toThrow()
  })
})
