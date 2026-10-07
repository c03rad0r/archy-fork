import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
const rpc = vi.hoisted(() => ({ call: vi.fn() }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: rpc }))
import { supportsRentalPlaybackOrigin, useRentalPurchaseBridge } from '../useRentalPurchaseBridge'
const offer = { title: 'Film', terms: { nodeDid: 'did:key:fixture', contentId: 'registered_fixture', sha256: 'a'.repeat(64), priceSats: 8, viewingSeconds: 3600 } }
const installed = { appId: 'indeedhub', appOrigins: ['https://node.test:7778'] }
const quote = { state: 'confirmation_required', network: 'mainnet', mint_url: 'https://original-mint.example.test', operation_id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', envelope_sha256: 'b'.repeat(64), wallet_debit_sats: 10, gross_token_sats: 9, seller_net_sats: 8, expires_at: 2000, seller_onion: 'fixture.onion' }
function fixture(consentBusy: () => boolean = () => false) {
 const child = { postMessage: vi.fn() }
 const bridge = useRentalPurchaseBridge({ consentBusy, appId: () => 'indeedhub', appUrl: () => 'https://node.test:7778/browse', frameWindow: () => child as unknown as Window })
 const send = (origin = 'https://node.test:7778', source: unknown = child) => bridge.handle({ data: { type: 'archipelago-rental-request', id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc', offer }, origin, source } as MessageEvent)
 return { bridge, child, send }
}
afterEach(() => vi.unstubAllGlobals())
beforeEach(() => { vi.stubGlobal('location', new URL('https://node.test')); vi.clearAllMocks(); rpc.call.mockImplementation(async ({ method }) => method === 'media.registration.context' ? installed : method === 'content.rental-purchase' ? quote : { playback_url: '/api/rental-playback/' + 'd'.repeat(64), expires_at: null }) })
describe('native rental confirmation', () => {
 it('cannot approve a quote without its saved network and mint', async()=>{
  const f=fixture();await f.send()
  rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:{...quote,mint_url:undefined})
  await f.bridge.review();expect(f.bridge.quote.value).toBeNull();expect(f.bridge.error.value).toContain('Invalid payment confirmation')
  const calls=rpc.call.mock.calls.length;await f.bridge.approve();expect(rpc.call).toHaveBeenCalledTimes(calls)
 })

 it('ignores foreign frames and origins before RPC', async () => { const f=fixture(); await f.send('https://foreign.test'); await f.send(undefined, {}); expect(rpc.call).not.toHaveBeenCalled() })
 it('requires review then confirmation of the exact debit', async () => {
  const f=fixture(); await f.send(); await f.bridge.approve(); expect(rpc.call).toHaveBeenCalledTimes(1)
  await f.bridge.review(); expect(f.bridge.phase.value).toBe('confirm')
  expect(rpc.call.mock.calls.find(([v]) => v.method==='content.rental-purchase')![0].params.consent).toBeUndefined()
  rpc.call.mockImplementation(async ({method})=>method==='media.registration.context'?installed:method==='content.rental-purchase'?{state:'entitled',operation_id:quote.operation_id}:{playback_url:'/api/rental-playback/'+'d'.repeat(64),expires_at:null})
  await f.bridge.approve()
  const calls=rpc.call.mock.calls.filter(([v])=>v.method==='content.rental-purchase')
  expect(calls[1]![0].params.consent).toEqual({operation_id:quote.operation_id,envelope_sha256:quote.envelope_sha256,wallet_debit_sats:10})
  expect(calls[1]![0].params.max_wallet_debit).toBe(10); expect(f.bridge.request.value).toBeNull()
  expect(f.child.postMessage.mock.lastCall![0].result.playback_url).toContain('/api/rental-playback/')
 })
 it('does not dispatch after closing during installation validation', async()=>{
  const f=fixture(); await f.send(); let release!:(value:unknown)=>void
  rpc.call.mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const work=f.bridge.review();f.bridge.cancel();release(installed);await work
  expect(rpc.call.mock.calls.some(([v])=>v.method==='content.rental-purchase')).toBe(false)
 })
 it('does not assign a delayed payment result to a replacement request', async()=>{
  const f=fixture();await f.send();await f.bridge.review();let release!:(value:unknown)=>void
  rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:new Promise(resolve=>{release=resolve}))
  const work=f.bridge.approve();await vi.waitFor(()=>expect(release).toBeTypeOf('function'))
  f.bridge.cancel();await f.send();release({state:'entitled',operation_id:quote.operation_id});await work
  expect(f.bridge.phase.value).toBe('review');expect(f.bridge.request.value).toEqual(offer)
  expect(rpc.call.mock.calls.some(([v])=>v.method==='content.playback-handle')).toBe(false)
 })
 it('retries recovery without reusing UI approval after an ambiguous response', async()=>{
  const f=fixture();await f.send();await f.bridge.review()
  rpc.call.mockImplementation(async({method})=>{if(method==='media.registration.context')return installed;throw Error('Response lost')})
  await f.bridge.approve();expect(f.bridge.phase.value).toBe('review');await f.bridge.review()
  expect(rpc.call.mock.calls.filter(([v])=>v.method==='content.rental-purchase').slice(-1)[0]![0].params.consent).toBeUndefined()
 })
 it('clears an unpaid quote only after acknowledged cancellation', async()=>{
  const f=fixture();await f.send();await f.bridge.review()
  rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:{state:'unknown'})
  await f.bridge.cancelUnpaid();expect(f.bridge.quote.value?.operation_id).toBe(quote.operation_id)
  rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:{state:'cancelled_unspent'})
  await f.bridge.cancelUnpaid();expect(f.bridge.quote.value).toBeNull()
  expect(rpc.call.mock.calls.filter(([v])=>v.method==='content.cancel-purchase').slice(-1)[0]![0].params).toEqual({onion:'fixture.onion',operation_id:quote.operation_id})
 })
 it('lease status does not open a purchase or confirm spending', async()=>{
  const f=fixture();rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:{expires_at:null})
  await f.bridge.handle({data:{type:'archipelago-rental-request',id:'cccccccc-cccc-4ccc-8ccc-cccccccccccc',action:'status',handle:'d'.repeat(64)},origin:'https://node.test:7778',source:f.child} as unknown as MessageEvent)
  expect(rpc.call.mock.calls.map(([v])=>v.method)).toEqual(['media.registration.context','content.playback-status'])
  expect(f.child.postMessage.mock.lastCall![0].result).toEqual({expires_at:null})
  expect(f.bridge.request.value).toBeNull()
 })

 it('rejects a rental during another native confirmation without contacting the wallet',async()=>{
  const f=fixture(()=>true);await f.send();expect(rpc.call).not.toHaveBeenCalled()
  expect(f.child.postMessage.mock.lastCall![0].error).toContain('other native confirmation')
  expect(f.bridge.request.value).toBeNull()
 })
 it('does not approve a reviewed quote while a signer confirmation owns the surface',async()=>{
  let busy=false;const f=fixture(()=>busy);await f.send();await f.bridge.review()
  const calls=rpc.call.mock.calls.length;busy=true;await f.bridge.approve()
  expect(rpc.call).toHaveBeenCalledTimes(calls);expect(f.bridge.phase.value).toBe('confirm')
  expect(f.bridge.error.value).toContain('other native confirmation')
 })
 it('drops a status response after the surface closes even if its WindowProxy is reused',async()=>{
  const f=fixture();let release!:(value:unknown)=>void
  rpc.call.mockImplementation(async({method})=>method==='media.registration.context'?installed:new Promise(resolve=>{release=resolve}))
  const work=f.bridge.handle({data:{type:'archipelago-rental-request',id:'cccccccc-cccc-4ccc-8ccc-cccccccccccc',action:'status',handle:'d'.repeat(64)},origin:'https://node.test:7778',source:f.child} as unknown as MessageEvent)
  await vi.waitFor(()=>expect(release).toBeTypeOf('function'))
  f.bridge.cancel();release({expires_at:100});await work
  expect(f.child.postMessage).not.toHaveBeenCalled()
 })
 it('does not dispatch when the active frame disappears during installation verification',async()=>{
  const child={postMessage:vi.fn()};let active=true
  const bridge=useRentalPurchaseBridge({appId:()=> 'indeedhub',appUrl:()=> 'https://node.test:7778/browse',frameWindow:()=>active?child as unknown as Window:null})
  await bridge.handle({data:{type:'archipelago-rental-request',id:'cccccccc-cccc-4ccc-8ccc-cccccccccccc',offer},origin:'https://node.test:7778',source:child} as unknown as MessageEvent)
  let release!:(value:unknown)=>void;rpc.call.mockImplementationOnce(()=>new Promise(resolve=>{release=resolve}))
  const work=bridge.review();active=false;bridge.cancel();release(installed);await work
  expect(rpc.call.mock.calls.some(([v])=>v.method==='content.rental-purchase')).toBe(false)
 })

 it('rejects cross-site rental before preparation or spending',async()=>{
  vi.stubGlobal('location',new URL('https://dashboard.onion'))
  const f=fixture();await f.send()
  expect(rpc.call).not.toHaveBeenCalled();expect(f.bridge.request.value).toBeNull()
  expect(f.child.postMessage.mock.lastCall![0].error).toContain('same LAN hostname')
 })
 it('permits same-host ports but rejects separate onions and mixed schemes',()=>{
  expect(supportsRentalPlaybackOrigin('https://node.test:7778','https://node.test')).toBe(true)
  expect(supportsRentalPlaybackOrigin('http://node.test:7778','http://node.test')).toBe(true)
  expect(supportsRentalPlaybackOrigin('http://app.onion','http://dashboard.onion')).toBe(false)
  expect(supportsRentalPlaybackOrigin('http://node.test:7778','https://node.test')).toBe(false)
 })

})
