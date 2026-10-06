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
})
