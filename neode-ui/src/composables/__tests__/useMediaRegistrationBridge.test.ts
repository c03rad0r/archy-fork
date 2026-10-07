import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
const rpc = vi.hoisted(() => ({ call: vi.fn() }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: rpc }))
import { installedOriginMatches, useMediaRegistrationBridge } from '../useMediaRegistrationBridge'

const intent = { version: 1 as const, requestId: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', nonce: 'a'.repeat(64),
  appAudience: 'fixture-installed-app', nodeDid: 'did:key:fixture', producer: 'b'.repeat(64), projectId: 'project',
  priceSats: 15, viewingSeconds: 3600, createdAt: 1000, expiresAt: 1600 }
const selection = { relative_path: 'Movies/film.mp4', payment_methods: ['cashu'] }
const installed = { appId: 'indeedhub', appAudience: intent.appAudience, nodeDid: intent.nodeDid, appOrigins: ['https://node.test:7778'] }
let sequence = 1
const id = () => `bbbbbbbb-bbbb-4bbb-8bbb-${String(sequence++).padStart(12, '0')}`
function fixture() {
  const child = { postMessage: vi.fn() }
  const bridge = useMediaRegistrationBridge({ appId: () => 'indeedhub', appUrl: () => 'https://node.test:7778/browse', frameWindow: () => child as unknown as Window })
  const send = (action: string, extra: Record<string, unknown> = {}, origin = 'https://node.test:7778', source: unknown = child) => bridge.handle({
    data: { type: 'archipelago-media-registration-request', id: id(), action, intent, ...extra }, origin, source,
  } as MessageEvent)
  return { child, bridge, send }
}
beforeEach(() => {
  localStorage.clear(); vi.clearAllMocks(); sequence = 1
  vi.spyOn(Date, 'now').mockReturnValue(1100_000)
  vi.stubGlobal('crypto', { randomUUID: id })
  rpc.call.mockImplementation(async ({ method }) => method === 'media.registration.context' ? installed : { requestId: intent.requestId, contentId: 'registered_fixture' })
})
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals() })

describe('native Cloud registration ownership and interrupted operation boundary', () => {
  it('ignores other windows/origins and checks installer scope before showing Cloud selection', async () => {
    const f = fixture()
    await f.send('select', {}, 'https://evil.test'); await f.send('select', {}, undefined, {})
    expect(rpc.call).not.toHaveBeenCalled(); expect(f.bridge.request.value).toBeNull()
    rpc.call.mockResolvedValue({ ...installed, appAudience: 'other-install' })
    await f.send('select')
    expect(f.bridge.request.value).toBeNull()
    expect(f.child.postMessage.mock.lastCall?.[0].error).toContain('installed IndeeHub')
  })
  it('saves exact owner approval before signature and never prepares changed file/terms', async () => {
    const f = fixture(); await f.send('select')
    expect(localStorage.length).toBe(0)
    f.bridge.approve(selection)
    const approved = f.child.postMessage.mock.lastCall![0].result
    expect(localStorage.length).toBe(1)
    expect(f.bridge.phase.value).toBe('signing')
    await f.send('submit', { selection: { ...selection, relative_path: 'private.mp4' }, approvalId: approved.approvalId })
    expect(rpc.call.mock.calls.filter(([v]) => v.method === 'media.registration.prepare')).toHaveLength(0)
    await f.send('submit', { selection, approvalId: approved.approvalId, producerEvent: { ...approved.event, pubkey: intent.producer, content: '{}' } })
    expect(rpc.call.mock.calls.filter(([v]) => v.method === 'media.registration.prepare')).toHaveLength(0)
  })
  it('resumes the exact saved event after reload and expiry without a new selection or timestamp', async () => {
    const first = fixture(); await first.send('select'); first.bridge.approve(selection)
    const original = first.child.postMessage.mock.lastCall![0].result
    first.bridge.dispose()
    vi.mocked(Date.now).mockReturnValue(1700_000)
    const resumed = fixture(); await resumed.send('resume')
    expect(resumed.child.postMessage.mock.lastCall![0].result).toEqual(original)
    const signed = { ...original.event, pubkey: intent.producer, id: 'c'.repeat(64), sig: 'd'.repeat(128) }
    await resumed.send('submit', { selection, approvalId: original.approvalId, producerEvent: signed })
    expect(rpc.call.mock.calls.filter(([v]) => v.method === 'media.registration.prepare')).toHaveLength(1)
    expect(resumed.bridge.request.value).toBeNull()
    await resumed.send('complete')
    expect(localStorage.length).toBe(0)
    await resumed.send('complete')
    expect(resumed.child.postMessage.mock.lastCall![0].result.completed).toBe(true)
  })
  it('allows same-operation signer cancellation retry but rejects replacement pending terms', async () => {
    const f = fixture(); await f.send('select'); f.bridge.approve(selection)
    const original = f.child.postMessage.mock.lastCall![0].result
    await f.send('resume')
    expect(f.child.postMessage.mock.lastCall![0].result).toEqual(original)
    await f.send('resume', { intent: { ...intent, priceSats: 99 } })
    expect(f.child.postMessage.mock.lastCall![0].error).toBeTruthy()
    expect(f.bridge.request.value!.intent.priceSats).toBe(15)
  })
  it('does not authorize preparation when owner approval cannot be persisted', async () => {
    const f = fixture(); await f.send('select')
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('storage full') })
    f.bridge.approve(selection)
    expect(f.bridge.phase.value).toBe('select')
    expect(f.bridge.error.value).toBe('storage full')
    expect(f.child.postMessage).not.toHaveBeenCalled()
  })
  it('reserves validation and cannot restore a cancelled selection after its response arrives', async () => {
    const f=fixture(); let release!:(value:unknown)=>void
    const implementation=rpc.call.getMockImplementation()!
    rpc.call.mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
    const work=f.send('select')
    expect(f.bridge.isBusy()).toBe(true)
    f.bridge.cancel()
    release(await implementation({method:'media.registration.context'})); await work
    expect(f.bridge.request.value).toBeNull();expect(f.bridge.isBusy()).toBe(false)
  })
  it('recovers resolution approval across reload and cannot use it to prepare a file', async () => {
    vi.mocked(Date.now).mockReturnValue(1700_000)
    const first = fixture(); await first.send('resolve')
    expect(first.bridge.phase.value).toBe('resolve')
    first.bridge.approveResolution()
    const original = first.child.postMessage.mock.lastCall![0].result
    expect(original.event.kind).toBe(27237)
    first.bridge.dispose()
    vi.mocked(Date.now).mockReturnValue(1900_000)
    const next = fixture(); await next.send('resolve')
    expect(next.child.postMessage.mock.lastCall![0].result).toEqual(original)
    const signed = { ...original.event, pubkey: intent.producer, id: 'c'.repeat(64), sig: 'd'.repeat(128) }
    await next.send('submit', { selection, approvalId: original.approvalId, producerEvent: signed })
    expect(rpc.call.mock.calls.filter(([v]) => v.method === 'media.registration.prepare')).toHaveLength(0)
    await next.send('resolve-submit', { approvalId: original.approvalId, producerEvent: signed })
    expect(rpc.call.mock.calls.filter(([v]) => v.method === 'media.registration.resolve')).toHaveLength(1)
    await next.send('complete')
    expect(localStorage.length).toBe(0)
  })

})

describe('installed registration origin mapping',()=>{
 it('keeps mapped loopback origins on the actual dashboard hostname and configured port',()=>{
  const actual=new URL(window.location.href);actual.port='7778'
  expect(installedOriginMatches(actual.origin,`${actual.protocol}//127.0.0.1:7778`)).toBe(true)
  expect(installedOriginMatches('http://foreign.test:7778','http://127.0.0.1:7778')).toBe(false)
  expect(installedOriginMatches(actual.origin,`${actual.protocol}//127.0.0.1:7779`)).toBe(false)
 })
})
