import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent, h, KeepAlive, nextTick, ref } from 'vue'
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
import { rpcClient } from '@/api/rpc-client'
import VpnDeviceSettings from './VpnDeviceSettings.vue'
const peer = { name: 'Phone', ip: '10.44.0.2/32', config: 'PRIVATE-LIST-SECRET' }
const connection = { qr_svg: '<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script><rect width="10" height="10" /></svg>', config: 'PRIVATE-EXPLICIT-CONFIG', peer_ip: '10.44.0.2/32' }
function view(verified = true) { return mount(VpnDeviceSettings, { props: { configured: true, managementVerified: verified, statusKnown: true } }) }
function button(w: ReturnType<typeof view>, label: string) { const b = w.findAll('button').find(b => b.text() === label); expect(b).toBeDefined(); return b! }
beforeEach(() => {
  vi.resetAllMocks()
  vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
    if (method === 'vpn.list-peers') return { peers: [peer] }
    if (method === 'vpn.peer-config' || method === 'vpn.create-peer') return connection
    if (method === 'vpn.remove-peer') return { removed: true }
    throw new Error(method)
  })
})
describe('Private device management', () => {
  it('loads saved device names without rendering private list data or making changes', async () => {
    const w = view(); await flushPromises()
    expect(w.text()).toContain('Phone'); expect(w.html()).not.toContain('PRIVATE-LIST-SECRET')
    expect(rpcClient.call).toHaveBeenCalledTimes(1); w.unmount()
  })
  it('does not fetch legacy private lists or connection details without verified capability', async () => {
    const w = view(false); await flushPromises()
    expect(button(w, 'Add device').attributes('disabled')).toBeDefined()
    expect(button(w, 'Refresh devices').attributes('disabled')).toBeDefined()
    expect(w.text()).toContain('Device details and changes are unavailable')
    expect(w.text()).not.toContain('Phone')
    expect(w.findAll('button').some(b => b.text() === 'Connection details')).toBe(false)
    await button(w, 'Refresh devices').trigger('click'); await w.get('form').trigger('submit'); await flushPromises()
    expect(rpcClient.call).not.toHaveBeenCalled()
    await w.setProps({ managementVerified: true, statusKnown: false }); await flushPromises()
    expect(rpcClient.call).not.toHaveBeenCalled()
    await w.setProps({ statusKnown: true }); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledTimes(1)
    expect(rpcClient.call).toHaveBeenCalledWith(expect.objectContaining({ method: 'vpn.list-peers', dedup: false }))
    w.unmount()
  })
  it('starts a fresh non-deduplicated list after capability recovery and ignores the prior generation', async () => {
    const replies: Array<(value: { peers: typeof peer[] }) => void> = []
    vi.mocked(rpcClient.call).mockImplementation(async () => new Promise(resolve => { replies.push(resolve) }))
    const w = view(); await flushPromises()
    await w.setProps({ managementVerified: false }); await flushPromises()
    await w.setProps({ managementVerified: true }); await flushPromises()
    expect(replies).toHaveLength(2)
    for (const [options] of vi.mocked(rpcClient.call).mock.calls) {
      expect(options).toEqual(expect.objectContaining({ method: 'vpn.list-peers', dedup: false }))
    }
    replies[1]!({ peers: [{ ...peer, name: 'Tablet' }] }); await flushPromises()
    expect(w.text()).toContain('Tablet')
    replies[0]!({ peers: [{ ...peer, name: 'Old Phone' }] }); await flushPromises()
    expect(w.text()).toContain('Tablet'); expect(w.text()).not.toContain('Old Phone')
    w.unmount()
  })
  it('clears private details and rejects an in-flight reveal when capability is lost', async () => {
    let resolve!: (value: typeof connection) => void
    const w = view(); await flushPromises()
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'vpn.list-peers') return { peers: [peer] }
      return new Promise(r => { resolve = r })
    })
    await button(w, 'Connection details').trigger('click')
    await w.setProps({ managementVerified: false }); await flushPromises()
    resolve(connection); await flushPromises()
    expect(w.find('svg').exists()).toBe(false)
    expect(w.text()).not.toContain('Phone')
    vi.mocked(rpcClient.call).mockClear()
    await button(w, 'Refresh devices').trigger('click'); await w.get('form').trigger('submit'); await flushPromises()
    expect(rpcClient.call).not.toHaveBeenCalled()
    w.unmount()
  })
  it('sanitizes explicit QR content and never renders the private config as text', async () => {
    const w = view(); await flushPromises(); await button(w, 'Connection details').trigger('click'); await flushPromises()
    expect(w.find('svg rect').exists()).toBe(true); expect(w.find('script').exists()).toBe(false)
    expect(w.html()).not.toContain('PRIVATE-EXPLICIT-CONFIG')
    await button(w, 'Hide details').trigger('click'); expect(w.find('svg').exists()).toBe(false); w.unmount()
  })
  it.each(['../escape', 'a/b', '.', 'bad\\path', 'Phone', 'phone'])('rejects unsafe or duplicate device name %s without mutation', async name => {
    const w = view(); await flushPromises(); await w.get('input').setValue(name)
    await w.get('form').trigger('submit'); await flushPromises()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([a]) => a.method === 'vpn.create-peer')).toHaveLength(0); w.unmount()
  })
  it('adds only after explicit submission, without automatic mutation retries', async () => {
    const w = view(); await flushPromises(); await w.get('input').setValue('Laptop')
    await w.get('form').trigger('submit'); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'vpn.create-peer', params: { name: 'Laptop' }, maxRetries: 0 })
    expect(w.text()).toContain('Connect Laptop'); w.unmount()
  })
  it('requires a successful refresh after an uncertain creation before retrying', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => { if (method === 'vpn.list-peers') return { peers: [] }; throw new Error('lost response') })
    const w = view(); await flushPromises(); await w.get('input').setValue('Laptop'); await w.get('form').trigger('submit'); await flushPromises()
    expect(w.text()).toContain('Refresh the list before trying again')
    await w.get('form').trigger('submit'); await flushPromises()
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([a]) => a.method === 'vpn.create-peer')).toHaveLength(1)
    await button(w, 'Refresh devices').trigger('click'); await flushPromises()
    expect(button(w, 'Add device').attributes('disabled')).toBeUndefined(); w.unmount()
  })
  it('requires removal confirmation and keeps devices when the backend does not confirm', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => method === 'vpn.list-peers' ? { peers: [peer] } : { removed: false })
    const w = view(); await flushPromises(); await w.get('[aria-label="Remove Phone"]').trigger('click')
    expect(vi.mocked(rpcClient.call).mock.calls.filter(([a]) => a.method === 'vpn.remove-peer')).toHaveLength(0)
    await button(w, 'Remove device').trigger('click'); await flushPromises()
    expect(w.text()).toContain('Could not confirm removal'); expect(w.find('[aria-label="Remove Phone"]').exists()).toBe(true); w.unmount()
  })
  it('keeps a failed list distinct from no devices and disables creation', async () => {
    vi.mocked(rpcClient.call).mockRejectedValue(new Error('offline'))
    const w = view(); await flushPromises()
    expect(w.text()).toContain('Could not load devices'); expect(w.text()).not.toContain('No devices have been added')
    expect(button(w, 'Add device').attributes('disabled')).toBeDefined(); w.unmount()
  })
  it('does not show copied until the clipboard succeeds', async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error('denied')) } })
    const w = view(); await flushPromises(); await button(w, 'Connection details').trigger('click'); await flushPromises()
    await button(w, 'Copy connection settings').trigger('click'); await flushPromises()
    expect(w.text()).toContain('Could not copy'); expect(w.text()).not.toContain('Copied'); w.unmount()
  })
  it('clears private details across cached navigation and ignores an old reveal response', async () => {
    let resolve!: (value: typeof connection) => void
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }) => {
      if (method === 'vpn.list-peers') return { peers: [peer] }
      return new Promise(r => { resolve = r })
    })
    const visible = ref(true)
    const Host = defineComponent({ setup: () => () => h(KeepAlive, null, { default: () => visible.value ? h(VpnDeviceSettings, { configured: true, managementVerified: true, statusKnown: true }) : h('div', 'Elsewhere') }) })
    const host = mount(Host); await flushPromises()
    const details = host.findAll('button').find(b => b.text() === 'Connection details')!
    await details.trigger('click')
    visible.value = false; await nextTick(); visible.value = true; await nextTick(); await flushPromises()
    resolve(connection); await flushPromises()
    expect(host.text()).not.toContain('Connect Phone'); expect(host.find('svg').exists()).toBe(false)
    host.unmount()
  })

  it('offers recovery for unfinished operations without revealing an unconfirmed connection', async () => {
    vi.mocked(rpcClient.call).mockResolvedValue({ peers: [{ ...peer, management_state: 'pending' }] })
    const w = view(); await flushPromises()
    expect(w.text()).toContain('Setup was not confirmed')
    expect(button(w, 'Connection details').attributes('disabled')).toBeDefined()
    expect(w.get('[aria-label="Remove Phone"]').attributes('disabled')).toBeUndefined()
    w.unmount()
  })

})
