import { mount, type VueWrapper } from '@vue/test-utils'
import { nextTick } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import AppLauncherOverlay from '../AppLauncherOverlay.vue'
import { useAppLauncherStore } from '@/stores/appLauncher'
vi.mock('@/stores/appLauncher', async () => {
  const { reactive } = await import('vue')
  const state = reactive({ isOpen: false, url: '', title: 'Custom app', showConsent: false, setNostrFrame: vi.fn(), close: vi.fn(), consentPhase: 'review', setNativeIdentityBusy: vi.fn(), registrationRequest: null, registrationPhase: 'select', registrationError: '', rentalRequest: null, rentalQuote: null, rentalPhase: 'review', rentalError: '' })
  return { useAppLauncherStore: () => state }
})
vi.mock('@/composables/useLightningRequired', () => ({ useLightningRequired: () => ({}) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
describe('legacy custom app slow loading', () => {
  let wrapper: VueWrapper
  beforeEach(() => { vi.useFakeTimers(); const store = useAppLauncherStore(); store.isOpen = false; store.url = ''; localStorage.clear() })
  afterEach(() => { wrapper?.unmount(); vi.useRealTimers() })
  async function render() {
    wrapper = mount(AppLauncherOverlay, { global: { stubs: { Teleport: false, Transition: false, AppLoadingScreen: true, NostrSignConsent: true, NostrIdentityPicker: true, PaymentSuccessPane: true } } })
    const store = useAppLauncherStore(); store.url = 'https://custom-app.invalid/'; store.isOpen = true
    await nextTick()
  }
  function iframe() { return document.querySelector('iframe')! }
  it('retains the frame after the deadline and recovers on its eventual load', async () => {
    await render(); const original = iframe()
    expect(original).not.toBeNull()
    await vi.advanceTimersByTimeAsync(16000); await nextTick()
    expect(iframe()).toBe(original)
    expect(document.querySelector('[role="status"]')?.textContent).toContain('Custom app is taking longer')
    original.dispatchEvent(new Event('load')); await nextTick()
    expect(iframe()).toBe(original)
    expect(document.querySelector('[role="status"]')).toBeNull()
  })
  it('allows explicit retry and dismissing the second slow-load notice without another navigation', async () => {
    await render(); const original = iframe()
    await vi.advanceTimersByTimeAsync(16000); await nextTick()
    const retry = document.querySelector('[role="status"] button') as HTMLButtonElement
    retry.click(); await nextTick()
    const retried = iframe(); expect(retried).not.toBe(original)
    await vi.advanceTimersByTimeAsync(16000); await nextTick()
    expect(iframe()).toBe(retried)
    ;(document.querySelector('[aria-label="Dismiss loading notice"]') as HTMLButtonElement).click(); await nextTick()
    expect(document.querySelector('[role="status"]')).toBeNull()
    expect(iframe()).toBe(retried)
  })
  it('does not leave a loading notice after closing the app', async () => {
    await render(); useAppLauncherStore().isOpen = false; await nextTick()
    await vi.advanceTimersByTimeAsync(16000); await nextTick()
    expect(document.querySelector('iframe')).toBeNull()
    expect(document.querySelector('[role="status"]')).toBeNull()
  })
})
