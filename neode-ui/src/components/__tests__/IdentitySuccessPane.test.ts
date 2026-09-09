import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import IdentitySuccessPane from '../IdentitySuccessPane.vue'

describe('IdentitySuccessPane', () => {
  it('shows the saved identity, relay coverage, event id, and identity animation', () => {
    const wrapper = mount(IdentitySuccessPane, {
      props: {
        identityName: 'Alice', eventId: 'abc123', accepted: 2, attempted: 3,
        relayNote: 'One relay rejected the event.',
      },
    })
    expect(wrapper.text()).toContain('IDENTITY UPDATED')
    expect(wrapper.text()).toContain('Alice is updated on this node.')
    expect(wrapper.text()).toContain('Published to 2/3 configured relays.')
    expect(wrapper.text()).toContain('abc123')
    expect(wrapper.text()).toContain('One relay rejected')
    expect(wrapper.find('.nostr-orb-success').exists()).toBe(true)
  })

  it('reports a local-only save without claiming relay publication', () => {
    const wrapper = mount(IdentitySuccessPane, {
      props: { identityName: 'Alice', accepted: 0, attempted: 0 },
    })
    expect(wrapper.text()).toContain('Saved locally; no relay publication was attempted.')
  })
})
