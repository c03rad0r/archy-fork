import { describe, expect, it, vi, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'

const getSpy = vi.fn()
const setSpy = vi.fn()
const toast = { success: vi.fn(), error: vi.fn(), info: vi.fn() }

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: (req: { method: string; params?: unknown }) => {
      if (req.method === 'fips.ssh-over-mesh.get') return getSpy(req)
      if (req.method === 'fips.ssh-over-mesh.set') return setSpy(req)
      if (req.method === 'fips.status') return Promise.resolve({ npub: 'npub1test' })
      return Promise.resolve({})
    },
  },
}))
vi.mock('@/composables/useToast', () => ({ useToast: () => toast }))

import SshOverMeshCard from '../SshOverMeshCard.vue'

function statusBody(overrides: Record<string, unknown> = {}) {
  return {
    enabled: false,
    sources: [],
    scope: 'any',
    preflights: { sshd_active: true, sshd_ipv6_listen: true, password_auth: false },
    ...overrides,
  }
}

describe('SshOverMeshCard', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    getSpy.mockResolvedValue(statusBody())
    setSpy.mockResolvedValue(statusBody({ enabled: true }))
  })

  it('renders preflights from the RPC state', async () => {
    getSpy.mockResolvedValue(statusBody())
    const wrapper = mount(SshOverMeshCard)
    await vi.dynamicImportSettled()
    await new Promise(r => setTimeout(r, 0))
    expect(getSpy).toHaveBeenCalledWith(expect.objectContaining({ method: 'fips.ssh-over-mesh.get' }))
    expect(wrapper.text()).toContain('sshd is running')
    expect(wrapper.text()).toContain('keys-only login')
  })

  it('flags the missing IPv6 listener honestly', async () => {
    getSpy.mockResolvedValue(statusBody({
      preflights: { sshd_active: true, sshd_ipv6_listen: false, password_auth: true },
    }))
    const wrapper = mount(SshOverMeshCard)
    await new Promise(r => setTimeout(r, 0))
    expect(wrapper.text()).toContain('does not listen on IPv6')
    expect(wrapper.text()).toContain('keys-only is the safer pairing')
  })

  it('demands the danger-zone confirmation before an unrestricted enable', async () => {
    const wrapper = mount(SshOverMeshCard)
    await new Promise(r => setTimeout(r, 0))
    await wrapper.find('button.path-action-button').trigger('click')
    // The confirmation modal Teleports to <body>; nothing applied yet.
    expect(document.body.textContent).toContain('Allow SSH from ANY mesh peer')
    expect(setSpy).not.toHaveBeenCalled()
    // Confirming applies the unrestricted rule.
    const confirm = [...document.body.querySelectorAll('button')]
      .find(b => (b.textContent ?? '').includes('I understand'))
    expect(confirm).toBeDefined()
    confirm!.dispatchEvent(new Event('click'))
    await new Promise(r => setTimeout(r, 0))
    await new Promise(r => setTimeout(r, 0))
    expect(setSpy).toHaveBeenCalledWith(expect.objectContaining({
      params: { enabled: true, sources: [] },
    }))
  })

  it('applies a restricted enable without the any-peer confirmation', async () => {
    // An existing restricted rule: turning it back on re-applies the list.
    getSpy.mockResolvedValue(statusBody({ enabled: false, sources: ['fd00::1'], scope: 'list' }))
    setSpy.mockResolvedValue(statusBody({ enabled: true, sources: ['fd00::1'], scope: 'list' }))
    const wrapper = mount(SshOverMeshCard)
    await new Promise(r => setTimeout(r, 0))
    await wrapper.find('button.path-action-button').trigger('click')
    expect(setSpy).toHaveBeenCalledWith(expect.objectContaining({
      params: { enabled: true, sources: ['fd00::1'] },
    }))
    expect(wrapper.text()).not.toContain('Allow SSH from ANY mesh peer')
  })
})
