import { mount } from '@vue/test-utils'
import { expect, it, vi } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import Web5QuickActions from '../Web5QuickActions.vue'
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
it('keeps Connect and Connected links outside the collapsed mobile controls', async () => {
  const router = createRouter({ history: createMemoryHistory(), routes: [{ name: 'federation', path: '/connections', component: { template: '<div />' } }] })
  const wrapper = mount(Web5QuickActions, { props: {
    showStagger: false, profitsBreakdown: null, networkingProfitsDisplay: '0 sats', userDid: null,
    didStatus: 'inactive', didCopied: false, creatingDid: false, dhtDid: null, dhtDidCopied: false,
    publishingDht: false, nostrRelayStats: null, connectedNodesCount: 0, detectedHwWallets: [],
  }, global: { plugins: [router] } })
  const entry = wrapper.get('[aria-label="Node connections"]')
  expect(entry.element.closest('.hidden')).toBeNull()
  expect(entry.findAll('a').map(a => a.attributes('href'))).toEqual(['/connections?view=discover', '/connections?view=connected'])
  expect(wrapper.text()).toContain('Show more')
  wrapper.unmount()
})
