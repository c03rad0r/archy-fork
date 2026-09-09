import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import NostrSignConsent from '../NostrSignConsent.vue'

const baseProps = {
  show: true,
  appName: 'GitWorkshop',
  method: 'signEvent',
  identityLabel: 'Personal',
  eventKind: 1621,
}

describe('NostrSignConsent', () => {
  it('renders as a contained absolute overlay, not a body teleport', () => {
    const wrapper = mount(NostrSignConsent, { props: baseProps })
    expect(wrapper.findComponent({ name: 'Teleport' }).exists()).toBe(false)
    expect(wrapper.find('.absolute.inset-0').exists()).toBe(true)
    expect(wrapper.text()).toContain('Approve this request?')
    expect(wrapper.text()).toContain('Git issue')
  })

  it('uses the shared identity animation while the node signs', () => {
    const wrapper = mount(NostrSignConsent, { props: { ...baseProps, phase: 'signing' } })
    expect(wrapper.text()).toContain('Signing locally…')
    expect(wrapper.find('.nostr-orb-loading').exists()).toBe(true)
    expect(wrapper.findAll('.nostr-orb-segment')).toHaveLength(48)
    expect(wrapper.text()).not.toContain('Approve')
  })

  it('shows a short in-app completion state', () => {
    const wrapper = mount(NostrSignConsent, { props: { ...baseProps, phase: 'success' } })
    expect(wrapper.text()).toContain('Request signed')
    expect(wrapper.find('.nostr-orb-success').exists()).toBe(true)
  })
})
