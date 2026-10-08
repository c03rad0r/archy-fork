import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const api = vi.hoisted(() => ({ status: vi.fn(), update: vi.fn(), dns: vi.fn(), generate: vi.fn() }))
const page = vi.hoisted(() => ({ name: 'external-access' }))
vi.mock('vue-router', () => ({ useRoute: () => page, RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('@/services/publishing', async (original) => ({ ...await original<typeof import('@/services/publishing')>(), publishing: api }))
import PublishingSetup from '../PublishingSetup.vue'

const state = () => ({ schema: 1, version: 2, connections: ['fips'], projects: {} })
beforeEach(() => {
  vi.clearAllMocks(); page.name = 'external-access'
  api.status.mockResolvedValue({ state: state(), fips_address: null, apps: [], publication_enabled: false, notice: 'Saving does not publish.' })
  api.update.mockResolvedValue({ state: { ...state(), version: 3 }, project_id: null })
})
describe('publishing setup', () => {
  it('loads choices from the node and saves multiple routes without activating them', async () => {
    const wrapper = mount(PublishingSetup); await flushPromises()
    const inputs = wrapper.findAll('input[type="checkbox"]')
    expect((inputs[0]!.element as HTMLInputElement).checked).toBe(true)
    await inputs[2]!.setValue(true)
    await wrapper.findAll('button').find(b => b.text() === 'Save connection choices')!.trigger('click')
    await flushPromises()
    expect(api.update).toHaveBeenCalledWith(2, { action: 'connections', routes: ['fips', 'tor'] })
    expect(wrapper.text()).toContain('Existing app access has not changed')
  })
  it('keeps a failed save visible and does not pretend it succeeded', async () => {
    api.update.mockRejectedValue(new Error('Reload before saving'))
    const wrapper = mount(PublishingSetup); await flushPromises()
    await wrapper.findAll('button').find(b => b.text() === 'Save connection choices')!.trigger('click'); await flushPromises()
    expect(wrapper.get('[role="alert"]').text()).toContain('Reload before saving')
    expect(wrapper.text()).not.toContain('Connection preferences saved')
  })
  it('isolates saved HTML and presents Nostr as an independent choice', async () => {
    page.name = 'publish-website'
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<script>parent.fetch("/rpc")</script>', routes: ['fips', 'nostr'], domain: null, revisions: [] } } }, apps: [], fips_address: 'fd00::1', publication_enabled: false, notice: 'Saving does not publish.' })
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(wrapper.get('iframe').attributes('sandbox')).toBe('')
    expect(wrapper.get('iframe').attributes('srcdoc')).toContain("default-src 'none'")
    expect(wrapper.findAll('input[type="checkbox"]')).toHaveLength(4)
    expect(wrapper.text()).toContain('reachability still needs verification')
  })
})
