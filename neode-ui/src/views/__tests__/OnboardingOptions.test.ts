import { describe, expect, it, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import OnboardingOptions from '../OnboardingOptions.vue'

const push = vi.fn(() => Promise.resolve())

vi.mock('vue-router', () => ({
  useRouter: () => ({ push }),
}))

vi.mock('@/composables/useNavSounds', () => ({
  playNavSound: vi.fn(),
}))

describe('OnboardingOptions', () => {
  it('shows only usable setup paths', () => {
    const wrapper = mount(OnboardingOptions)

    expect(wrapper.text()).toContain('Fresh Start')
    expect(wrapper.text()).toContain('Restore from Seed')
    expect(wrapper.text()).not.toContain('Connect Existing')
    expect(wrapper.text()).not.toContain('Coming Soon')
  })

  it('continues to fresh seed generation by default', async () => {
    push.mockClear()
    const wrapper = mount(OnboardingOptions)

    await wrapper.get('button.path-action-button').trigger('click')

    expect(push).toHaveBeenCalledWith('/onboarding/seed')
  })

  it('routes restore choice to seed restore', async () => {
    push.mockClear()
    const wrapper = mount(OnboardingOptions)

    const restoreButton = wrapper.findAll('button').find((button) => button.text().includes('Restore from Seed'))
    expect(restoreButton).toBeDefined()

    await restoreButton!.trigger('click')
    await wrapper.get('button.path-action-button').trigger('click')

    expect(push).toHaveBeenCalledWith('/onboarding/seed-restore')
  })
})
