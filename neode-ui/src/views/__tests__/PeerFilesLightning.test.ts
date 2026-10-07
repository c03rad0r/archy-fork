import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import PeerFiles from '../PeerFiles.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn(), federationListNodes: vi.fn(), payLightningInvoice: vi.fn() } }))
vi.mock('@/composables/useAudioPlayer', () => ({ useAudioPlayer: () => ({ play: vi.fn() }) }))
const hash = 'a'.repeat(64)
const item = { id: 'paid-file', filename: 'bought.txt', mime_type: 'text/plain', size_bytes: 4, description: '', access: { paid: { price_sats: 5, accepted: ['lightning', 'ecash'] } } }
const receiptKey = 'peer-file-lightning:peer.onion:paid-file'
const cashuQuoteFixture = { state: 'confirmation_required', network: 'mainnet', mint_url: 'https://original-mint.example.test', operation_id: '12345678-1234-4234-8234-123456789abc', envelope_sha256: 'b'.repeat(64), gross_token_sats: 6, seller_net_sats: 5, wallet_debit_sats: 7, expires_at: 2_000_000_000 }
const download = vi.fn()
// Models the node's guarded native payment RPC, not a browser-side LND call.
const nativePay = vi.fn()
async function open() {
  const wrapper = mount(PeerFiles, { props: { peerId: 'peer.onion' }, global: { plugins: [createPinia()], stubs: { Teleport: true } } })
  await flushPromises()
  // Drive the actual component payment handlers, asserting RPC effects rather
  // than a duplicate implementation of the payment state machine.
  const vm = (wrapper.vm as any).$.setupState
  vm.openPayModal(item)
  await flushPromises()
  return { wrapper, vm }
}
beforeEach(() => {
  localStorage.clear(); vi.clearAllMocks()
  vi.mocked(rpcClient.federationListNodes).mockResolvedValue({ nodes: [] } as never)
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'content.purchase') return cashuQuoteFixture
    if (method === 'content.invoice-create') return { operation_id:'11111111-1111-4111-8111-111111111111', bolt11: 'ln-test', payment_hash: hash, price_sats: 5,external_exposure:false }
    if (method === 'content.invoice-pay') return nativePay()
    if (method === 'content.download-peer-invoice' || method === 'content.invoice-download') return download()
    return { items: [], attempts: [], attempt: null }
  })
  nativePay.mockReset().mockResolvedValue({ status: 'succeeded' } as never)
  download.mockReset().mockResolvedValue({error:'Original payment is unresolved'})
})
describe('Lightning file delivery recovery', () => {
  it('opens a confirmed on-chain delivery from HTTP cache without another payment', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.onchain-status') return { paid: true }
      if (method === 'content.download-peer-onchain') return { owned: true, mime_type: 'video/mp4', size_bytes: 200000000 }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    await vm.pollOnchain('bc1test')
    expect(vm.viewerUrl).toBe('/api/peer-content/peer.onion/paid-file')
    expect(vm.viewerMime).toBe('video/mp4')
    const calls = vi.mocked(rpcClient.call).mock.calls.map(([call]) => call)
    expect(calls.find(call => call.method === 'content.download-peer-onchain')?.params).toMatchObject({ cache_only: true, address: 'bc1test', filename: 'bought.txt' })
    expect(calls.some(call => ['lnd.sendcoins', 'content.request-onchain'].includes(call.method))).toBe(false)
    wrapper.unmount()
  })
  it('opens a cached ecash purchase without transferring base64 into the UI', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.purchase') return { state: 'delivered', owned: true, owned_content_id: item.id, mime_type: 'video/mp4', size_bytes: 200000000 }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    await vm.confirmEcashPay()
    expect(vm.viewerUrl).toBe('/api/peer-content/peer.onion/paid-file')
    expect(vm.viewerMime).toBe('video/mp4')
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.purchase')![0].params).toMatchObject({ content_id: item.id, max_wallet_debit: Number.MAX_SAFE_INTEGER })
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.purchase')![0].maxRetries).toBe(1)
    wrapper.unmount()
  })
  it('does not issue a duplicate ecash purchase while delivery is pending', async () => {
    let finish!: (value: unknown) => void
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.purchase') return await new Promise(resolve => { finish = resolve })
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    const first = vm.confirmEcashPay()
    await vm.confirmEcashPay()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.purchase')).toHaveLength(1)
    finish({ error: 'Delivery needs recovery; do not pay again' })
    await first
    expect(vm.purchaseError).toContain('do not pay again')
    wrapper.unmount()
  })
  it('retries delivery after a seller rejection without paying or requesting another invoice', async () => {
    download.mockResolvedValue({ error: 'Seller has not registered this payment yet' })
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ payment_hash: hash })
    vm.closePayModal(); vm.openPayModal(item)
    await vm.payWithLightning()
    expect(nativePay).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.invoice-create')).toHaveLength(1)
    expect(download).toHaveBeenCalledTimes(2)
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.invoice-download')![0].params).toEqual({onion:'peer.onion',content_id:item.id,operation_id:'11111111-1111-4111-8111-111111111111'})
    wrapper.unmount()
  })
  it('recovers the same node-owned payment operation after an ambiguous reply', async () => {
    nativePay.mockRejectedValueOnce(new Error('Connection lost')).mockResolvedValue({status:'succeeded'})
    download.mockResolvedValue({ error: 'Pending' })
    const first = await open(); await first.vm.payWithLightning(); first.wrapper.unmount()
    const second = await open(); await second.vm.payWithLightning()
    // RPC recovery may be repeated; the node dispatch-once engine owns spending.
    const requests=vi.mocked(rpcClient.call).mock.calls.filter(([v])=>v.method==='content.invoice-pay')
    expect(requests).toHaveLength(2)
    expect(requests.map(([v])=>v.params)).toEqual([requests[0]![0].params,requests[0]![0].params])
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v])=>v.method==='content.invoice-create')).toHaveLength(1)
    expect(download).toHaveBeenCalledTimes(1)
    second.wrapper.unmount()
  })
  it('opens cached delivery through HTTP without a base64 file in the response', async () => {
    download.mockResolvedValue({ owned: true, mime_type: 'video/mp4', size_bytes: 206165161 })
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(vm.viewerUrl).toBe('/api/peer-content/peer.onion/paid-file')
    expect(vm.viewerMime).toBe('video/mp4')
    expect(localStorage.getItem(receiptKey)).toBeNull()
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.invoice-download')![0].params).toMatchObject({operation_id:'11111111-1111-4111-8111-111111111111'})
    expect(nativePay).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
  it('never pays again when the saved receipt is corrupt', async () => {
    localStorage.setItem(receiptKey, '{broken')
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.invoice-create')).toHaveLength(0)
    wrapper.unmount()
  })
  it('keeps QR recovery on the saved Lightning payment instead of creating another rail', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 }))
    const { wrapper, vm } = await open()
    vm.openQrPay(); await flushPromises()
    expect(vm.payMode).toBe('qr')
    expect(vm.qrTab).toBe('lightning')
    vm.selectQrTab('onchain'); await flushPromises()
    expect(vm.qrTab).toBe('lightning')
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => ['content.invoice-create', 'content.request-onchain'].includes(v.method))).toHaveLength(0)
    wrapper.unmount()
  })
  it('does not send payment if the recovery record cannot be saved', async () => {
    const { wrapper, vm } = await open()
    const save = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Storage full') })
    await vm.payWithLightning()
    expect(nativePay).not.toHaveBeenCalled()
    save.mockRestore(); wrapper.unmount()
  })
  it('reopens ecash after a returned terminal failure while retaining failed-attempt history', async () => {
    nativePay.mockResolvedValue({ status: 'failed', failure_reason: 'Insufficient channel balance' } as never)
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'wallet.ecash-balance' ? { cashu_sats: 10 } : original(args))
    const { wrapper, vm } = await open()
    vm.openPayModal({ ...item, access: { paid: { price_sats: 5, accepted: ['lightning', 'ecash'] } } })
    await vm.payWithLightning(); await flushPromises()
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'failed', payment_hash: hash })
    expect(wrapper.text()).toContain('Pay from this node’s ecash wallet')
    await vm.prepareEcashPay()
    expect(vm.ecashPlan.chosen).toBe('cashu')
    expect(nativePay).toHaveBeenCalledTimes(1)
    expect(download).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('resolves an old-backend exception using definitive LND state', async () => {
    nativePay.mockRejectedValue(new Error('Payment failed: Insufficient channel balance'))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'failed', failure_reason: 'Insufficient channel balance' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'failed' })
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'lnd.paymentstatus')![0].params).toEqual({ payment_hash: hash })
    expect(download).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('does not treat a legacy receipt of unknown exposure as canceled from local LND failure', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 }))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'failed', failure_reason: 'No route' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => v.method === 'content.invoice-create')).toBe(false)
    wrapper.unmount()
  })
  it('keeps ambiguous attempts recoverable and prevents another payment method', async () => {
    nativePay.mockRejectedValue(new Error('Connection lost'))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'unknown' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'pending' })
    await vm.prepareEcashPay()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    await vm.confirmEcashPay(); await vm.payOnchain()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => ['content.download-peer-paid', 'content.request-onchain', 'lnd.sendcoins'].includes(v.method))).toBe(false)
    expect(nativePay).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })

  it('returns known in-flight recovery promptly without a long delivery call or another payment', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'saved', payment_hash: hash, price_sats: 5, state: 'pending' }))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'pending' } : original(args))
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(vm.lnPaying).toBe(false)
    expect(vm.lnError).toContain('close this window')
    expect(vi.mocked(rpcClient.call).mock.calls.some(([c]) => ['content.invoice-create', 'content.download-peer-invoice'].includes(c.method))).toBe(false)
    expect(nativePay).not.toHaveBeenCalled()
    vm.closePayModal()
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'pending' })
    wrapper.unmount()
  })
  it('requests a fresh QR invoice after a confirmed failed local attempt', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'failed-old', payment_hash: hash, price_sats: 5, state: 'failed' }))
    const { wrapper, vm } = await open()
    await vm.payWithInvoice()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([c]) => c.method === 'content.invoice-create')).toHaveLength(1)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ bolt11: 'ln-test', state: 'pending' })
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })

// Append inside the existing PeerFilesLightning describe; uses real mounted component.
it('does not pay when an invoice arrives after closing and reopening the same file', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.invoice-create'
    ? await new Promise(resolve => { reply = resolve }) : { items: [], attempts: [], attempt: null })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  await flushPromises()
  expect(typeof reply).toBe('function')
  vm.closePayModal(); vm.openPayModal(item)
  reply({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 })
  await pending
  expect(nativePay).not.toHaveBeenCalled()
  expect(vm.payItem.id).toBe(item.id)
  expect(vm.lnPaying).toBe(false)
  expect(localStorage.getItem(receiptKey)).toBeNull()
  wrapper.unmount()
})
it('retains a late invoice for its original file without changing another file modal', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.invoice-create'
    ? await new Promise(resolve => { reply = resolve }) : { items: [], attempts: [], attempt: null })
  const { wrapper, vm } = await open()
  const pending = vm.payWithInvoice()
  await flushPromises()
  expect(typeof reply).toBe('function')
  await vm.payWithInvoice()
  expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.invoice-create')).toHaveLength(1)
  vm.closePayModal(); vm.openPayModal({ ...item, id: 'second-file' })
  reply({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 })
  await pending
  expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ payment_hash: hash })
  expect(vm.payItem.id).toBe('second-file')
  expect(vm.invoiceData).toBeNull()
  expect(vm.lnReceipt).toBeNull()
  expect(vm.invoiceWaiting).toBe(false)
  wrapper.unmount()
})
it('keeps a new file balance preparation busy when an older preparation finishes', async () => {
  const replies: ((value: unknown) => void)[] = []
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'wallet.ecash-balance'
    ? await new Promise(resolve => { replies.push(resolve) }) : { items: [], attempts: [], attempt: null })
  const { wrapper, vm } = await open()
  const first = vm.prepareEcashPay()
  await flushPromises()
  expect(replies).toHaveLength(1)
  vm.closePayModal(); vm.openPayModal({ ...item, id: 'second-file' })
  const second = vm.prepareEcashPay()
  await flushPromises()
  expect(replies).toHaveLength(2)
  replies[0]!({ cashu_sats: 100 })
  await first
  expect(vm.ecashPreparing).toBe(true)
  expect(vm.ecashPlan).toBeNull()
  replies[1]!({ cashu_sats: 20 })
  await second
  expect(vm.ecashPreparing).toBe(false)
  expect(vm.ecashPlan.cashu).toBe(20)
  wrapper.unmount()
})
it('cannot deliver a late paid invoice into another file modal', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'content.invoice-status') return { paid: true }
    if (method === 'content.download-peer-invoice') return await new Promise(resolve => { reply = resolve })
    return { items: [], attempts: [], attempt: null }
  })
  const { wrapper, vm } = await open()
  const invoice = { bolt11: 'ln-test', payment_hash: hash, price_sats: 5 }
  vm.invoiceData = invoice
  const pending = vm.pollInvoice({ item, onion: 'peer.onion', invoice, generation: vm.paymentGeneration })
  await flushPromises()
  vm.closePayModal(); vm.openPayModal({ ...item, id: 'second-file' })
  reply({ owned: true, mime_type: 'video/mp4' })
  await pending
  expect(vm.viewerUrl).toBeNull()
  expect(vm.payItem.id).toBe('second-file')
  expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'succeeded' })
  wrapper.unmount()
})
it('persists a dispatched Lightning result after closing without changing another file', async () => {
  let reply!: (value: unknown) => void
  nativePay.mockImplementation(async () => await new Promise<unknown>(resolve => { reply = resolve }) as never)
  download.mockResolvedValue({ error: 'Delivery remains recoverable' })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  await flushPromises()
  expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'pending' })
  vm.closePayModal(); vm.openPayModal({ ...item, id: 'second-file' })
  reply({ status: 'succeeded' })
  await pending
  expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'succeeded' })
  expect(vm.payItem.id).toBe('second-file')
  expect(vm.viewerUrl).toBeNull()
  expect(vm.lnReceipt).toBeNull()
  expect(vm.lnError).toBe('')
  expect(nativePay).toHaveBeenCalledTimes(1)
  wrapper.unmount()
})
it('does not dispatch Lightning when its invoice arrives after component unmount', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.invoice-create'
    ? await new Promise(resolve => { reply = resolve }) : { items: [], attempts: [], attempt: null })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  await flushPromises()
  expect(typeof reply).toBe('function')
  wrapper.unmount()
  reply({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 })
  await pending
  expect(nativePay).not.toHaveBeenCalled()
  expect(localStorage.getItem(receiptKey)).toBeNull()
})
it('retains already dispatched Lightning evidence after unmount without opening playback', async () => {
  let reply!: (value: unknown) => void
  nativePay.mockImplementation(async () => await new Promise<unknown>(resolve => { reply = resolve }) as never)
  download.mockResolvedValue({ owned: true, mime_type: 'video/mp4' })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  await flushPromises()
  wrapper.unmount()
  reply({ status: 'succeeded' })
  await pending
  expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'succeeded' })
  expect(vm.viewerUrl).toBeNull()
})

  it('does not create an address, attempt or onchain send for a 545-sat file', async () => {
    const { wrapper, vm } = await open()
    vm.openPayModal({ ...item, access: { paid: { price_sats: 545, accepted: ['onchain', 'lightning', 'ecash'] } } })
    await vm.payOnchain()
    await vm.loadOnchainQr()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call]) => ['content.request-onchain', 'lnd.sendcoins'].includes(call.method))).toBe(false)
    expect(vm.paymentActionBusy).toBe(false)
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(vm.lnError).toContain('546')
    wrapper.unmount()
  })
  it('allows the exact 546-sat boundary and never dispatches a changed seller amount', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.request-onchain') return { address: 'bc1test', amount_sats: 547 }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    vm.openPayModal({ ...item, access: { paid: { price_sats: 546, accepted: ['onchain', 'lightning', 'ecash'] } } })
    await vm.payOnchain()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.request-onchain')).toHaveLength(1)
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => v.method === 'lnd.sendcoins')).toBe(false)
    expect(vm.lnError).toContain('changed the payment amount')
    expect(vm.paymentActionBusy).toBe(false)
    wrapper.unmount()
  })

  it.each([
    ['copyInvoice', 'invoiceData', 'invoiceCopied', { bolt11: 'invoice-one', payment_hash: hash, price_sats: 5 }],
    ['copyOnchain', 'onchainData', 'onchainCopied', { address: 'address-one', amount_sats: 546 }],
  ] as const)('ignores late %s completion after reopening the same file', async (method, data, copied, value) => {
    let resolve!: () => void
    const writeText = vi.fn(() => new Promise<void>(done => { resolve = done }))
    const previous = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } })
    const { wrapper, vm } = await open()
    try {
      vm[data] = value
      const pending = vm[method]()
      vm.closePayModal(); vm.openPayModal(item)
      vm[data] = { ...value }
      resolve(); await pending
      expect(vm[copied]).toBe(false)
      expect(writeText).toHaveBeenCalledTimes(1)
    } finally {
      wrapper.unmount()
      if (previous) Object.defineProperty(navigator, 'clipboard', previous)
      else Reflect.deleteProperty(navigator, 'clipboard')
    }
  })
  it.each([
    ['copyInvoice', 'invoiceData', 'invoiceCopied', { bolt11: 'invoice-one', payment_hash: hash, price_sats: 5 }],
    ['copyOnchain', 'onchainData', 'onchainCopied', { address: 'address-one', amount_sats: 546 }],
  ] as const)('does not let the old %s timer reset a reopened modal', async (method, data, copied, value) => {
    const previous = Object.getOwnPropertyDescriptor(navigator, 'clipboard')
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockResolvedValue(undefined) } })
    const { wrapper, vm } = await open()
    vi.useFakeTimers()
    try {
      vm[data] = value
      await vm[method]()
      expect(vm[copied]).toBe(true)
      await vi.advanceTimersByTimeAsync(1000)
      vm.closePayModal(); vm.openPayModal(item)
      vm[data] = { ...value }
      await vm[method]()
      await vi.advanceTimersByTimeAsync(1000)
      expect(vm[copied]).toBe(true)
      await vi.advanceTimersByTimeAsync(1000)
      expect(vm[copied]).toBe(false)
    } finally {
      wrapper.unmount(); vi.useRealTimers()
      if (previous) Object.defineProperty(navigator, 'clipboard', previous)
      else Reflect.deleteProperty(navigator, 'clipboard')
    }
  })

})


describe('Seller-authoritative external invoice lifecycle', () => {
  it('unlocks alternate methods only after the seller confirms the saved external invoice canceled and unpaid', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5, state: 'pending' }))
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'lnd.paymentstatus') throw new Error('Unknown external payment')
      if (method === 'content.invoice-status') return { paid: false, state: 'canceled', can_switch_method: true }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ payment_hash: hash, state: 'failed' })
    expect(vm.lnError).toContain('seller confirmed')
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call]) => ['content.invoice-create', 'content.download-peer-invoice'].includes(call.method))).toBe(false)
    wrapper.unmount()
  })
  it.each([
    { paid: false },
    { paid: false, state: 'open', expires_at: 1, can_switch_method: false },
    { paid: false, state: 'unknown', can_switch_method: false },
    { paid: false, state: 'canceled' },
    { paid: false, state: 'accepted', can_switch_method: true },
  ])('retains the saved attempt when seller status does not prove terminal cancellation: %j', async response => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5, state: 'pending' }))
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'lnd.paymentstatus') throw new Error('Unknown external payment')
      if (method === 'content.invoice-status') return response
      if (method === 'content.download-peer-invoice') return { error: 'Payment is still pending' }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ payment_hash: hash, state: 'pending' })
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call]) => call.method === 'content.invoice-create')).toBe(false)
    wrapper.unmount()
  })
})

describe('Durable node Cashu purchases', () => {
  it('shows the exact quoted debit and confirms its immutable terms without the legacy send RPC', async () => {
    let purchaseCalls = 0
    vi.mocked(rpcClient.call).mockImplementation(async ({method}) => {
      if (method === 'content.purchase') return ++purchaseCalls === 1 ? cashuQuoteFixture : {state:'delivered',owned:true,owned_content_id:item.id,mime_type:'text/plain'}
      return {items:[],attempts:[],attempt:null}
    })
    const {wrapper,vm}=await open()
    await vm.prepareEcashPay()
    expect(vm.cashuQuote.wallet_debit_sats).toBe(7)
    expect(wrapper.text()).toContain('fees/rounding: 2 sats')
    expect(purchaseCalls).toBe(1)
    await vm.confirmEcashPay()
    const calls=vi.mocked(rpcClient.call).mock.calls.map(([call])=>call)
    const requests=calls.filter(call=>call.method==='content.purchase')
    expect(requests[0]?.params).not.toHaveProperty('consent')
    expect(requests[1]?.params).toMatchObject({max_wallet_debit:7,consent:{operation_id:cashuQuoteFixture.operation_id,envelope_sha256:cashuQuoteFixture.envelope_sha256,wallet_debit_sats:7}})
    expect(calls.some(call=>call.method==='content.download-peer-paid')).toBe(false)
    expect(vm.viewerUrl).toContain('/paid-file')
    wrapper.unmount()
  })
  it('recovers after a lost submit reply and remount without confirming another payment', async () => {
    let count=0
    vi.mocked(rpcClient.call).mockImplementation(async ({method})=>{
      if (method==='content.purchase') {
        count++
        if(count===1)return cashuQuoteFixture
        if(count===2)throw new Error('Connection lost; original purchase saved')
        return {state:'delivered',owned:true,owned_content_id:item.id,mime_type:'text/plain'}
      }
      return {items:[],attempts:[],attempt:null}
    })
    const first=await open();await first.vm.prepareEcashPay();await first.vm.confirmEcashPay();first.wrapper.unmount()
    const next=await open();expect(next.vm.hasBlockingCashuPurchase).toBe(true)
    await next.vm.payWithLightning();expect(nativePay).not.toHaveBeenCalled()
    await next.vm.prepareEcashPay()
    const requests=vi.mocked(rpcClient.call).mock.calls.filter(([call])=>call.method==='content.purchase')
    expect(requests).toHaveLength(3)
    expect(requests[2]?.[0].params).not.toHaveProperty('consent')
    expect(next.vm.viewerUrl).toContain('/paid-file')
    next.wrapper.unmount()
  })
  it('keeps the original operation until seller cancellation acknowledgement, then permits a new quote', async () => {
    let canceled=false, cancelCalls=0
    vi.mocked(rpcClient.call).mockImplementation(async ({method})=>{
      if(method==='content.purchase')return canceled?{...cashuQuoteFixture,operation_id:'87654321-1234-4234-8234-123456789abc'}:cashuQuoteFixture
      if(method==='content.cancel-purchase'){
        if(++cancelCalls===1)throw new Error('Cancellation reply lost')
        canceled=true;return {state:'cancelled_unspent',operation_id:cashuQuoteFixture.operation_id}
      }
      return {items:[],attempts:[],attempt:null}
    })
    const {wrapper,vm}=await open();await vm.prepareEcashPay();await vm.cancelCashuPurchase()
    expect(vm.hasBlockingCashuPurchase).toBe(true)
    await vm.payWithLightning();expect(nativePay).not.toHaveBeenCalled()
    await vm.cancelCashuPurchase();expect(vm.hasBlockingCashuPurchase).toBe(false)
    await vm.prepareEcashPay();expect(vm.cashuQuote.operation_id).not.toBe(cashuQuoteFixture.operation_id)
    wrapper.unmount()
  })
  it('finds a node-owned pending purchase after browser state loss before another rail can dispatch', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({method})=>{
      if(method==='content.payment-status')return {attempts:[{operation_id:cashuQuoteFixture.operation_id,state:'token_prepared_settlement_unconfirmed'}]}
      if(method==='content.purchase')return {state:'delivered',owned:true,owned_content_id:item.id,mime_type:'text/plain'}
      return {items:[],attempts:[],attempt:null}
    })
    const {wrapper,vm}=await open();await vm.payWithLightning()
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call])=>call.method==='content.invoice-create')).toBe(false)
    await vm.prepareEcashPay();expect(vm.viewerUrl).toContain('/paid-file')
    wrapper.unmount()
  })
})

describe('Malformed browser Cashu marker recovery', () => {
  const marker='peer-file-cashu:peer.onion:paid-file'
  it('queries node-owned state without consent, archives the malformed marker and restores the authoritative quote', async () => {
    localStorage.setItem(marker,'{original broken marker')
    const {wrapper,vm}=await open()
    expect(vm.cashuRecoveryError).toBe(true)
    await vm.prepareEcashPay()
    expect(localStorage.getItem(`${marker}:unreadable`)).toBe('{original broken marker')
    expect(JSON.parse(localStorage.getItem(marker)!)).toMatchObject({quote:cashuQuoteFixture,dispatched:false})
    expect(vm.cashuRecoveryError).toBe(false)
    expect(vm.hasBlockingCashuPurchase).toBe(true)
    const calls=vi.mocked(rpcClient.call).mock.calls.filter(([call])=>call.method==='content.purchase')
    expect(calls).toHaveLength(1)
    expect(calls[0]?.[0].params).not.toHaveProperty('consent')
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('keeps original malformed data and all replacement methods blocked when node recovery is unavailable', async () => {
    localStorage.setItem(marker,'{original broken marker')
    vi.mocked(rpcClient.call).mockImplementation(async ({method})=>{
      if(method==='content.purchase')throw new Error('Node purchase journal is unavailable')
      return {items:[],attempts:[],attempt:null}
    })
    const {wrapper,vm}=await open();await vm.prepareEcashPay();await vm.payWithLightning()
    expect(localStorage.getItem(marker)).toBe('{original broken marker')
    expect(localStorage.getItem(`${marker}:unreadable`)).toBeNull()
    expect(vm.hasBlockingCashuPurchase).toBe(true)
    expect(vm.purchaseError).toContain('journal is unavailable')
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})


describe('External invoice recovery retains terminal settlement', () => {
  it('reopening a paid invoice QR never downgrades the receipt or requests another invoice', async () => {
    const settled = { bolt11: 'ln-original', payment_hash: hash, price_sats: 5, state: 'succeeded' }
    localStorage.setItem(receiptKey, JSON.stringify(settled))
    const { wrapper, vm } = await open()
    await vm.payWithInvoice()
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject(settled)
    expect(vm.lnReceipt.state).toBe('succeeded')
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => v.method === 'content.invoice-create')).toBe(false)
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})


describe('Durable external invoice ownership', () => {
  it('recovers a browser-lost invoice from node storage and blocks other rails', async()=>{
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-attempt'?{attempt:{operation_id:'11111111-1111-4111-8111-111111111111',price_sats:5,external_exposure:true,status:{payment_hash:hash,bolt11:'ln-original',state:'issued',can_switch_method:false}}}:original(args))
    const {wrapper,vm}=await open();await flushPromises();await vm.prepareEcashPay();await vm.payOnchain()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({bolt11:'ln-original',external_exposure:true})
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v])=>['content.invoice-create','lnd.sendcoins','content.purchase'].includes(v.method))).toBe(false)
    wrapper.unmount()
  })
  it('local LND failure cannot release an externally displayed invoice',async()=>{
    localStorage.setItem(receiptKey,JSON.stringify({bolt11:'ln-external',payment_hash:hash,price_sats:5,origin:'native',external_exposure:true,state:'pending'}))
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='lnd.paymentstatus'?{status:'failed'}:args.method==='content.invoice-status'?{paid:false,state:'open',can_switch_method:false}:original(args))
    const {wrapper,vm}=await open();await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(true);expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('settlement wins a cancellation reply and keeps paid-file recovery',async()=>{
    localStorage.setItem(receiptKey,JSON.stringify({operation_id:'11111111-1111-4111-8111-111111111111',bolt11:'ln-external',payment_hash:hash,price_sats:5,origin:'external',external_exposure:true,state:'pending'}))
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-cancel'?{paid:true,state:'settled',can_switch_method:false}:original(args))
    const {wrapper,vm}=await open();await vm.cancelExternalInvoice()
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({state:'succeeded'})
    expect(vm.hasBlockingLightningReceipt).toBe(true);expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})


describe('Succeeded invoice reconciliation',()=>{
  it('retains a succeeded browser receipt when node invoice metadata has not reached settled yet',async()=>{
    const operation='11111111-1111-4111-8111-111111111111'
    localStorage.setItem(receiptKey,JSON.stringify({operation_id:operation,payment_hash:hash,price_sats:5,origin:'native',external_exposure:false,state:'succeeded'}))
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-attempt'?{attempt:{operation_id:operation,price_sats:5,external_exposure:false,status:{payment_hash:hash,bolt11:null,state:'issued',can_switch_method:false}}}:original(args))
    const {wrapper,vm}=await open()
    expect(vm.lnReceipt.state).toBe('succeeded')
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({state:'succeeded'})
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('restores node-proven native success after browser storage is lost',async()=>{
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-attempt'?{attempt:{operation_id:'11111111-1111-4111-8111-111111111111',price_sats:5,external_exposure:false,native_succeeded:true,status:{payment_hash:hash,bolt11:null,state:'issued',can_switch_method:false}}}:original(args))
    const {wrapper,vm}=await open()
    expect(vm.lnReceipt.state).toBe('succeeded')
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})


describe('Damaged supplemental invoice receipt',()=>{
  it('reconciles only from an authoritative saved node operation and preserves the damaged bytes',async()=>{
    localStorage.setItem(receiptKey,'{damaged receipt')
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-attempt'?{attempt:{operation_id:'11111111-1111-4111-8111-111111111111',price_sats:5,external_exposure:true,status:{payment_hash:hash,bolt11:'ln-original',state:'issued',can_switch_method:false}}}:original(args))
    const {wrapper,vm}=await open()
    expect(localStorage.getItem(`${receiptKey}:unreadable`)).toBe('{damaged receipt')
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({payment_hash:hash,state:'pending',external_exposure:true})
    expect(vm.lnReceiptReadError).toBe(false)
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('retains damaged bytes and blocks replacement if the original node lookup fails',async()=>{
    localStorage.setItem(receiptKey,'{damaged receipt')
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>{if(args.method==='content.invoice-attempt')throw Error('Original journal unavailable');return original(args)})
    const {wrapper,vm}=await open();await vm.payWithLightning()
    expect(localStorage.getItem(receiptKey)).toBe('{damaged receipt')
    expect(localStorage.getItem(`${receiptKey}:unreadable`)).toBeNull()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(nativePay).not.toHaveBeenCalled()
    wrapper.unmount()
  })
})


it('recovers a saved incomplete invoice request without paying or exposing its BOLT11',async()=>{
  let recovered=false
  const operation='11111111-1111-4111-8111-111111111111'
  const original=vi.mocked(rpcClient.call).getMockImplementation()!
  vi.mocked(rpcClient.call).mockImplementation(async args=>{
    if(args.method==='content.invoice-attempt')return {attempt:{operation_id:operation,price_sats:5,external_exposure:false,status:recovered?{payment_hash:hash,bolt11:null,state:'issued',can_switch_method:false}:null}}
    if(args.method==='content.invoice-create'){recovered=true;return {operation_id:operation,state:'open',payment_hash:hash}}
    return original(args)
  })
  const {wrapper,vm}=await open();expect(vm.lnReceiptReadError).toBe(true)
  await vm.recoverOriginalInvoice()
  expect(vm.lnReceiptReadError).toBe(false)
  expect(vi.mocked(rpcClient.call).mock.calls.find(([call])=>call.method==='content.invoice-create')![0].params).toEqual({onion:'peer.onion',content_id:item.id,operation_id:operation,external_exposure:false})
  expect(nativePay).not.toHaveBeenCalled();expect(vm.invoiceData).toBeNull()
  expect(vm.lnReceipt).toMatchObject({operation_id:operation,state:'pending',external_exposure:false})
  wrapper.unmount()
})

describe('Explicit retry of a confirmed native-only failure',()=>{
  const oldOperation='11111111-1111-4111-8111-111111111111'
  const nextOperation='22222222-2222-4222-8222-222222222222'
  const nextHash='b'.repeat(64)
  it('does not retry on reopen; the explicit action pays only the new node-owned operation',async()=>{
    localStorage.setItem(receiptKey,JSON.stringify({operation_id:oldOperation,payment_hash:hash,price_sats:5,origin:'native',external_exposure:false,state:'failed'}))
    const original=vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args=>args.method==='content.invoice-retry-native'?{operation_id:nextOperation,payment_hash:nextHash,price_sats:5,external_exposure:false,state:'open'}:original(args))
    download.mockResolvedValue({error:'Delivery still pending'})
    const {wrapper,vm}=await open()
    expect(nativePay).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call])=>call.method==='content.invoice-retry-native')).toBe(false)
    await vm.retryNativeLightning()
    expect(vi.mocked(rpcClient.call).mock.calls.find(([call])=>call.method==='content.invoice-retry-native')![0].params).toMatchObject({operation_id:oldOperation,price_sats:5})
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([call])=>call.method==='content.invoice-pay').map(([call])=>call.params)).toEqual([{onion:'peer.onion',content_id:item.id,operation_id:nextOperation}])
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({operation_id:nextOperation,payment_hash:nextHash,state:'succeeded'})
    wrapper.unmount()
  })
  it('does not create a native retry for an invoice exposed to another wallet',async()=>{
    localStorage.setItem(receiptKey,JSON.stringify({operation_id:oldOperation,payment_hash:hash,price_sats:5,bolt11:'ln-external',origin:'external',external_exposure:true,state:'failed'}))
    const {wrapper,vm}=await open();await vm.retryNativeLightning()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call])=>call.method==='content.invoice-retry-native')).toBe(false)
    expect(nativePay).not.toHaveBeenCalled();wrapper.unmount()
  })
  it('a late failed reply from the old attempt cannot overwrite a replacement created in another window',async()=>{
    let resolve!: (value:unknown)=>void
    nativePay.mockImplementation(async()=>await new Promise(value=>{resolve=value}))
    const {wrapper,vm}=await open();const old=vm.payWithLightning();await flushPromises()
    expect(resolve).toBeTypeOf('function')
    const replacement={operation_id:nextOperation,payment_hash:nextHash,price_sats:5,origin:'native',external_exposure:false,state:'pending'}
    localStorage.setItem(receiptKey,JSON.stringify(replacement))
    resolve({status:'failed',failure_reason:'Old attempt failed'})
    await old
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toEqual(replacement)
    expect(vm.lnReceipt.operation_id).toBe(nextOperation)
    expect(vm.lnError).not.toContain('Old attempt failed')
    wrapper.unmount()
  })
})


describe('Supported peer-file ecash choices', () => {
  it('does not offer or spend Ark even when its wallet alone covers the file', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'wallet.ecash-balance') return { cashu_sats: 0, fedimint_sats: 0, ark_sats: 1000 }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    vm.openPayModal({ ...item, access: { paid: { price_sats: 5, accepted: ['fedimint'] } } })
    await flushPromises()
    await vm.prepareEcashPay()
    expect(vm.ecashPlan.chosen).toBeNull()
    expect(vm.ecashPlan.total).toBe(0)
    expect(vm.purchaseError).toContain('Ark cannot pay for peer files yet')
    expect(wrapper.findAll('button').some(button => button.text().includes('Ark'))).toBe(false)
    await vm.selectEcashBackend('ark')
    expect(vm.ecashPlan.chosen).toBeNull()
    // A stale or injected unsupported selection must also fail before RPC.
    vm.ecashPlan.chosen = 'ark'
    await vm.confirmEcashPay()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([call]) => call.method === 'content.download-peer-paid')).toBe(false)
    wrapper.unmount()
  })
})


describe('Unknown on-chain verification', () => {
  it('shows verification errors without downloading, paying or creating another address', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.onchain-status') return { paid: false, status: 'unknown', error: 'Exact outputs unavailable; do not pay again.' }
      return { items: [], attempts: [], attempt: null }
    })
    const { wrapper, vm } = await open()
    await vm.pollOnchain('original-address')
    expect(vm.lnError).toContain('do not pay again')
    const calls = vi.mocked(rpcClient.call).mock.calls.map(([call]) => call.method)
    expect(calls).not.toContain('content.download-peer-onchain')
    expect(calls).not.toContain('content.request-onchain')
    expect(calls).not.toContain('lnd.sendcoins')
    wrapper.unmount()
  })
})
