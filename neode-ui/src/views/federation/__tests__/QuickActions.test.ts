import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import QuickActions from '../QuickActions.vue'

describe('QuickActions federation sync', () => {
  it('shows bounded progress context while sync is running', () => {
    const wrapper = mount(QuickActions, {
      props: {
        generatingInvite: false,
        inviteType: 'trusted',
        inviteCode: '',
        syncing: true,
        syncNodeCount: 3,
      },
    })

    const button = wrapper.get('[data-testid="federation-sync-button"]')
    expect(button.attributes('disabled')).toBeDefined()
    const status = button.get('[role="status"]')
    expect(status.attributes('aria-live')).toBe('polite')
    expect(status.text()).toContain('Syncing 3 nodes')
    expect(status.find('.animate-spin').exists()).toBe(true)
  })

  it('keeps the action concise for a single node', () => {
    const wrapper = mount(QuickActions, {
      props: {
        generatingInvite: false,
        inviteType: 'trusted',
        inviteCode: '',
        syncing: true,
        syncNodeCount: 1,
      },
    })

    expect(wrapper.get('[data-testid="federation-sync-button"]').text()).toContain('Syncing 1 node…')
  })
})
