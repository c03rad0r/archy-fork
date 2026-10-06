import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick } from 'vue'
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import CompanionIntroOverlay from '../CompanionIntroOverlay.vue'
import { companionIntroRequested } from '@/composables/useCompanionIntro'
vi.mock('qrcode', () => ({ toDataURL: vi.fn().mockResolvedValue('data:image/png;base64,fixture') }))
vi.mock('@/utils/openExternal', () => ({ isCompanionApp: () => false }))
vi.mock('@/composables/useDemoIntro', () => ({ IS_DEMO: false, DEMO_PASSWORD: '' }))
vi.mock('@/composables/useCompanionRelease', async () => {
  const { ref } = await import('vue')
  return { companionRelease: ref({ versionName: 'fixture', versionCode: 1 }), companionReleaseMarker: () => 'fixture', loadCompanionRelease: vi.fn().mockResolvedValue(undefined) }
})
vi.mock('@/stores/loginTransition', () => ({ useLoginTransitionStore: () => ({ introCinematicPlaying: false }) }))
vi.mock('@/stores/server', () => ({ useServerStore: () => ({}) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
describe('automatic companion offer during app use', () => {
  let wrapper: VueWrapper
  beforeEach(() => { vi.useFakeTimers(); localStorage.clear(); companionIntroRequested.value = false })
  afterEach(() => { wrapper?.unmount(); vi.useRealTimers() })
  function render(deferAutomatic = false) { wrapper = mount(CompanionIntroOverlay, { props: { deferAutomatic }, global: { stubs: { Teleport: true, Transition: false } } }) }
  it('waits until the app closes before showing its automatic offer', async () => {
    render(true); await nextTick(); await vi.advanceTimersByTimeAsync(10000)
    expect(wrapper.find('[aria-label="Close companion modal"]').exists()).toBe(false)
    await wrapper.setProps({ deferAutomatic: false }); await vi.advanceTimersByTimeAsync(2500)
    expect(wrapper.find('[aria-label="Close companion modal"]').exists()).toBe(true)
    await wrapper.setProps({ deferAutomatic: true }); await vi.advanceTimersByTimeAsync(3000)
    expect(wrapper.find('[aria-label="Close companion modal"]').exists()).toBe(false)
  })
  it('honors manual opening while an app is active, without reopening after dismissal', async () => {
    render(true); await nextTick()
    companionIntroRequested.value = true; await nextTick()
    await wrapper.get('[aria-label="Close companion modal"]').trigger('click')
    await vi.advanceTimersByTimeAsync(12000)
    expect(wrapper.find('[aria-label="Close companion modal"]').exists()).toBe(false)
  })
  it('clears deferred timers when the dashboard is unmounted', async () => {
    render(true); await nextTick(); await vi.advanceTimersByTimeAsync(1000)
    wrapper.unmount(); await vi.advanceTimersByTimeAsync(15000)
    expect(vi.getTimerCount()).toBe(0)
  })
})
