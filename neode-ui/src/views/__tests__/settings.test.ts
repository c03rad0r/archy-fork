import { describe, it, expect } from 'vitest'
import { shallowMount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import Settings from '../Settings.vue'

describe('Settings View', () => {
  it('renders AccountSection and SystemSection', () => {
    setActivePinia(createPinia())
    const wrapper = shallowMount(Settings)
    expect(wrapper.findComponent({ name: 'AccountSection' }).exists()).toBe(true)
    expect(wrapper.findComponent({ name: 'SystemSection' }).exists()).toBe(true)
  })
})
