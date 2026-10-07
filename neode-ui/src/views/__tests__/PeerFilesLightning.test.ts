import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia } from 'pinia'
import PeerFiles from '../PeerFiles.vue'
import { rpcClient } from '@/api/rpc-client'
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn(), federationListNodes: vi.fn(), payLightningInvoice: vi.fn() } }))
vi.mock('@/composables/useAudioPlayer', () => ({ useAudioPlayer: () => ({ play: vi.fn() }) }))
const hash = 'a'.repeat(64)
const item = { id: 'paid-file', filename: 'bought.txt', mime_type: 'text/plain', size_bytes: 4, description: '', access: { paid: { price_sats: 5, accepted: ['lightning'] } } }
const receiptKey = 'peer-file-lightning:peer.onion:paid-file'
const download = vi.fn()
async function open() {
  const wrapper = mount(PeerFiles, { props: { peerId: 'peer.onion' }, global: { plugins: [createPinia()], stubs: { Teleport: true } } })
  await flushPromises()
  // Drive the actual component payment handlers, asserting RPC effects rather
  // than a duplicate implementation of the payment state machine.
  const vm = (wrapper.vm as any).$.setupState
  vm.openPayModal(item)
  return { wrapper, vm }
}
beforeEach(() => {
  localStorage.clear(); vi.clearAllMocks()
  vi.mocked(rpcClient.federationListNodes).mockResolvedValue({ nodes: [] } as never)
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'content.request-invoice') return { bolt11: 'ln-test', payment_hash: hash, price_sats: 5 }
    if (method === 'content.download-peer-invoice') return download()
    return { items: [] }
  })
  vi.mocked(rpcClient.payLightningInvoice).mockResolvedValue({ status: 'succeeded' } as never)
})
describe('Lightning file delivery recovery', () => {
  it('opens a confirmed on-chain delivery from HTTP cache without another payment', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.onchain-status') return { paid: true }
      if (method === 'content.download-peer-onchain') return { owned: true, mime_type: 'video/mp4', size_bytes: 200000000 }
      return { items: [] }
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
      if (method === 'content.download-peer-paid') return { owned: true, mime_type: 'video/mp4', size_bytes: 200000000 }
      return { items: [] }
    })
    const { wrapper, vm } = await open()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    await vm.confirmEcashPay()
    expect(vm.viewerUrl).toBe('/api/peer-content/peer.onion/paid-file')
    expect(vm.viewerMime).toBe('video/mp4')
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.download-peer-paid')![0].params).toMatchObject({ cache_only: true, method: 'cashu' })
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.download-peer-paid')![0].maxRetries).toBe(1)
    wrapper.unmount()
  })
  it('does not issue a duplicate ecash purchase while delivery is pending', async () => {
    let finish!: (value: unknown) => void
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'content.download-peer-paid') return await new Promise(resolve => { finish = resolve })
      return { items: [] }
    })
    const { wrapper, vm } = await open()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    const first = vm.confirmEcashPay()
    await vm.confirmEcashPay()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.download-peer-paid')).toHaveLength(1)
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
    expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.request-invoice')).toHaveLength(1)
    expect(download).toHaveBeenCalledTimes(2)
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.download-peer-invoice')![0].params).toMatchObject({ payment_hash: hash, filename: 'bought.txt', price_sats: 5 })
    wrapper.unmount()
  })
  it('restores an uncertain payment on a newly mounted page and only checks/downloads', async () => {
    vi.mocked(rpcClient.payLightningInvoice).mockRejectedValue(new Error('Connection lost'))
    download.mockResolvedValue({ error: 'Pending' })
    const first = await open(); await first.vm.payWithLightning(); first.wrapper.unmount()
    const second = await open(); await second.vm.payWithLightning()
    expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
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
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'content.download-peer-invoice')![0].params).toMatchObject({ cache_only: true })
    expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
    wrapper.unmount()
  })
  it('never pays again when the saved receipt is corrupt', async () => {
    localStorage.setItem(receiptKey, '{broken')
    const { wrapper, vm } = await open()
    await vm.payWithLightning()
    expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.request-invoice')).toHaveLength(0)
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
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => ['content.request-invoice', 'content.request-onchain'].includes(v.method))).toHaveLength(0)
    wrapper.unmount()
  })
  it('does not send payment if the recovery record cannot be saved', async () => {
    const { wrapper, vm } = await open()
    const save = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('Storage full') })
    await vm.payWithLightning()
    expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
    save.mockRestore(); wrapper.unmount()
  })
  it('reopens ecash after a returned terminal failure while retaining failed-attempt history', async () => {
    vi.mocked(rpcClient.payLightningInvoice).mockResolvedValue({ status: 'failed', failure_reason: 'Insufficient channel balance' } as never)
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
    expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
    expect(download).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('resolves an old-backend exception using definitive LND state', async () => {
    vi.mocked(rpcClient.payLightningInvoice).mockRejectedValue(new Error('Payment failed: Insufficient channel balance'))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'failed', failure_reason: 'Insufficient channel balance' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'failed' })
    expect(vi.mocked(rpcClient.call).mock.calls.find(([v]) => v.method === 'lnd.paymentstatus')![0].params).toEqual({ payment_hash: hash })
    expect(download).not.toHaveBeenCalled()
    wrapper.unmount()
  })
  it('resolves an already stuck receipt without another payment or invoice', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 }))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'failed', failure_reason: 'No route' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(false)
    expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => v.method === 'content.request-invoice')).toBe(false)
    wrapper.unmount()
  })
  it('keeps ambiguous attempts recoverable and prevents another payment method', async () => {
    vi.mocked(rpcClient.payLightningInvoice).mockRejectedValue(new Error('Connection lost'))
    const original = vi.mocked(rpcClient.call).getMockImplementation()!
    vi.mocked(rpcClient.call).mockImplementation(async args => args.method === 'lnd.paymentstatus' ? { status: 'unknown' } : original(args))
    const { wrapper, vm } = await open(); await vm.payWithLightning()
    expect(vm.hasBlockingLightningReceipt).toBe(true)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'pending' })
    await vm.prepareEcashPay()
    vm.ecashPlan = { cashu: 10, fedimint: 0, ark: 0, total: 10, chosen: 'cashu' }
    await vm.confirmEcashPay(); await vm.payOnchain()
    expect(vi.mocked(rpcClient.call).mock.calls.some(([v]) => ['content.download-peer-paid', 'content.request-onchain', 'lnd.sendcoins'].includes(v.method))).toBe(false)
    expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
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
    expect(vi.mocked(rpcClient.call).mock.calls.some(([c]) => ['content.request-invoice', 'content.download-peer-invoice'].includes(c.method))).toBe(false)
    expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
    vm.closePayModal()
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ state: 'pending' })
    wrapper.unmount()
  })
  it('requests a fresh QR invoice after a confirmed failed local attempt', async () => {
    localStorage.setItem(receiptKey, JSON.stringify({ bolt11: 'failed-old', payment_hash: hash, price_sats: 5, state: 'failed' }))
    const { wrapper, vm } = await open()
    await vm.payWithInvoice()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([c]) => c.method === 'content.request-invoice')).toHaveLength(1)
    expect(JSON.parse(localStorage.getItem(receiptKey)!)).toMatchObject({ bolt11: 'ln-test', state: 'pending' })
    expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
    wrapper.unmount()
  })

// Append inside the existing PeerFilesLightning describe; uses real mounted component.
it('does not pay when an invoice arrives after closing and reopening the same file', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.request-invoice'
    ? await new Promise(resolve => { reply = resolve }) : { items: [] })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  vm.closePayModal(); vm.openPayModal(item)
  reply({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 })
  await pending
  expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
  expect(vm.payItem.id).toBe(item.id)
  expect(vm.lnPaying).toBe(false)
  expect(localStorage.getItem(receiptKey)).toBeNull()
  wrapper.unmount()
})
it('retains a late invoice for its original file without changing another file modal', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.request-invoice'
    ? await new Promise(resolve => { reply = resolve }) : { items: [] })
  const { wrapper, vm } = await open()
  const pending = vm.payWithInvoice()
  await vm.payWithInvoice()
  expect(vi.mocked(rpcClient.call).mock.calls.filter(([v]) => v.method === 'content.request-invoice')).toHaveLength(1)
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
    ? await new Promise(resolve => { replies.push(resolve) }) : { items: [] })
  const { wrapper, vm } = await open()
  const first = vm.prepareEcashPay()
  vm.closePayModal(); vm.openPayModal({ ...item, id: 'second-file' })
  const second = vm.prepareEcashPay()
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
    return { items: [] }
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
  vi.mocked(rpcClient.payLightningInvoice).mockImplementation(async () => await new Promise<unknown>(resolve => { reply = resolve }) as never)
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
  expect(rpcClient.payLightningInvoice).toHaveBeenCalledTimes(1)
  wrapper.unmount()
})
it('does not dispatch Lightning when its invoice arrives after component unmount', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'content.request-invoice'
    ? await new Promise(resolve => { reply = resolve }) : { items: [] })
  const { wrapper, vm } = await open()
  const pending = vm.payWithLightning()
  wrapper.unmount()
  reply({ bolt11: 'ln-test', payment_hash: hash, price_sats: 5 })
  await pending
  expect(rpcClient.payLightningInvoice).not.toHaveBeenCalled()
  expect(localStorage.getItem(receiptKey)).toBeNull()
})
it('retains already dispatched Lightning evidence after unmount without opening playback', async () => {
  let reply!: (value: unknown) => void
  vi.mocked(rpcClient.payLightningInvoice).mockImplementation(async () => await new Promise<unknown>(resolve => { reply = resolve }) as never)
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
      return { items: [] }
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
