import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
const api = vi.hoisted(() => ({ status: vi.fn(), update: vi.fn(), dns: vi.fn(), generate: vi.fn(), verifyHttps: vi.fn() }))
const page = vi.hoisted(() => ({ name: 'external-access' }))
const appStore = vi.hoisted(() => ({ data: { 'package-data': {} as Record<string, unknown> } }))
vi.mock('@/stores/app', () => ({ useAppStore: () => appStore }))
vi.mock('vue-router', () => ({ useRoute: () => page, RouterLink: { props: ['to'], template: '<a :href="to"><slot /></a>' } }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
vi.mock('@/services/publishing', async (original) => ({ ...await original<typeof import('@/services/publishing')>(), publishing: api }))
import PublishingSetup from '../PublishingSetup.vue'
import { rpcClient } from '@/api/rpc-client'

const state = () => ({ schema: 1, version: 2, connections: ['fips'], projects: {} })
beforeEach(() => {
  vi.clearAllMocks(); page.name = 'external-access'
  appStore.data['package-data'] = {}
  api.status.mockResolvedValue({ state: state(), fips_address: null, apps: [], publication_enabled: false, notice: 'Saving does not publish.' })
  api.update.mockResolvedValue({ state: { ...state(), version: 3 }, project_id: null })
})
describe('publishing setup', () => {
  it('checks public HTTPS only on request and clears verification when the domain changes', async () => {
    page.name = 'publish-website'
    api.status.mockResolvedValue({ state: { ...state(), projects: { site: { id: 'site', name: 'Site', draft: '<h1>Public</h1>', routes: ['public-web'], domain: { hostname: 'www.example.com', destination: '8.8.8.8' }, revisions: [], fips_publication: { html: '<h1>Public</h1>', port: 32000 } } } }, apps: [], fips_address: 'fd00::1', publication_enabled: true, notice: '' })
    api.verifyHttps.mockResolvedValue({ hostname: 'www.example.com', sha256: 'synthetic', checked_at: '2026-10-08T00:00:00Z' })
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(api.verifyHttps).not.toHaveBeenCalled()
    await wrapper.findAll('button').find(b => b.text() === 'Check public HTTPS')!.trigger('click'); await flushPromises()
    expect(api.verifyHttps).toHaveBeenCalledWith('site', 2)
    expect(wrapper.text()).toContain('valid TLS and exact published content')
    await wrapper.get('input[placeholder="www.yourdomain.com"]').setValue('other.example.com')
    expect(wrapper.text()).not.toContain('valid TLS and exact published content')
  })
  it('creates only an explicit app-scoped grant and supports revocation without showing other credentials', async () => {
    api.status.mockResolvedValue({ state: state(), fips_address: null, apps: [{ id: 'nextcloud', name: 'Nextcloud', port: 8080, guest_access: true }], grants: [{ id: 'external:test:Guest', label: 'Guest', apps: ['nextcloud'], expires_at: 2000000000 }], notice: '' })
    vi.mocked(rpcClient.call).mockResolvedValue({ id: 'external:test:Guest', token: 'synthetic-test-token', app_id: 'nextcloud', expires_at: 2000000000 })
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(rpcClient.call).not.toHaveBeenCalled()
    await wrapper.get('select').setValue('nextcloud')
    await wrapper.findAll('button').find(b => b.text() === 'Create app-only access')!.trigger('click'); await flushPromises()
    expect(rpcClient.call).toHaveBeenCalledWith({ method: 'publishing.access-create', params: { app_id: 'nextcloud', label: 'Guest', hours: 24 }, maxRetries: 0 })
    expect(wrapper.text()).toContain('synthetic-test-token')
    await wrapper.findAll('button').find(b => b.text() === 'Revoke access')!.trigger('click'); await flushPromises()
    expect(rpcClient.call).toHaveBeenLastCalledWith({ method: 'publishing.access-revoke', params: { id: 'external:test:Guest' }, maxRetries: 0 })
    expect(wrapper.text()).not.toContain('synthetic-test-token')
  })
  it('offers catalog installation and skips it when Blossom is already installed', async () => {
    page.name = 'publish-website'
    const wrapper = mount(PublishingSetup); await flushPromises()
    expect(wrapper.get('[data-testid="blossom-setup"]').text()).toContain('Install Blossom')
    wrapper.unmount()
    appStore.data['package-data'].blossom = { state: 'installed' }
    const installed = mount(PublishingSetup); await flushPromises()
    expect(installed.get('[data-testid="blossom-setup"]').text()).toContain('skip installation')
    expect(installed.find('a[href="/dashboard/marketplace/blossom"]').exists()).toBe(false)
  })
  it('loads choices from the node and saves multiple routes without activating them', async () => {
    const wrapper = mount(PublishingSetup); await flushPromises()
    const inputs = wrapper.findAll('input[type="checkbox"][value]')
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
    expect(wrapper.findAll('input[type="checkbox"][value]')).toHaveLength(4)
    expect(wrapper.text()).toContain('reachability still needs verification')
  })
})
