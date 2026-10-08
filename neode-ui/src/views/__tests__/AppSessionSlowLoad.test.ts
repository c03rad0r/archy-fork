import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import AppSession from '../AppSession.vue'
import { useAppStore } from '@/stores/app'
vi.mock('vue-router', () => ({ useRoute: () => ({ params: { appId: 'indeedhub' }, query: {}, fullPath: '/dashboard/app-session/indeedhub' }), useRouter: () => ({ replace: vi.fn(() => Promise.resolve()), push: vi.fn(() => Promise.resolve()) }) }))
vi.mock('@/stores/appLauncher', () => ({ useAppLauncherStore: () => ({ panelAppId: null, mediaSessionGeneration: () => null, retainMediaSession: () => false }) }))
vi.mock('@/stores/app', async () => {
  const { reactive } = await import('vue')
  const state = reactive({ data: { 'package-data': {} } })
  return { useAppStore: () => state }
})
vi.mock('@/stores/screensaver', () => ({ useScreensaverStore: () => ({ suppress: vi.fn(), resume: vi.fn() }) }))
vi.mock('../appSession/useAppIdentity', () => ({ useAppIdentity: () => ({ onIdentitySelected: vi.fn(), onIframeLoadIdentity: vi.fn(), handleIdentityRequest: vi.fn(), getStoredIdentity: () => null, cancelIdentitySelection: vi.fn() }) }))
vi.mock('@/api/rpc-client', () => ({ rpcClient: { call: vi.fn() } }))
describe('AppSession slow navigation and readiness changes', () => {
  let wrapper: VueWrapper
  beforeEach(() => {
    vi.useFakeTimers(); setActivePinia(createPinia()); localStorage.clear()
    Object.defineProperty(window, 'innerWidth', { value: 390, configurable: true })
    Object.defineProperty(window, 'location', { value: { hostname: '192.0.2.10', origin: 'http://192.0.2.10', protocol: 'http:' }, configurable: true })
    useAppStore().data = { 'package-data': { indeedhub: { state: 'running', 'ui-ready': true, manifest: { id: 'indeedhub', title: 'IndeeHub' } } } } as never
  })
  afterEach(() => { wrapper?.unmount(); vi.useRealTimers() })
  function render() {
    // Keep real Teleport: its test stub freezes the dynamic slot props here.
    // Control frame load events separately; AppSessionFrame tests cover its DOM.
    wrapper = mount(AppSession, { global: { stubs: { Teleport: false, AppSessionHeader: true, AppSessionFrame: { name: 'AppSessionFrame', props: ['appUrl','loading','iframeBlocked','slowLoad','uiReadyBlocked','refreshKey'], emits: ['iframeLoad','iframeError','refresh'], template: '<div><iframe v-if="!iframeBlocked && !uiReadyBlocked" :key="refreshKey" /></div>' }, NostrIdentityPicker: true, NostrSignConsent: true, MobileGamepad: true, AppLoadingScreen: true } } })
    return wrapper.getComponent({ name: 'AppSessionFrame' })
  }
  function readiness(value: boolean) { useAppStore().data!['package-data'].indeedhub!['ui-ready'] = value }
  it('keeps a slow iframe alive past the deadline and accepts its eventual load', async () => {
    const frame = render(), original = frame.get('iframe').element
    await vi.advanceTimersByTimeAsync(13000); await nextTick()
    expect(frame.props('iframeBlocked')).toBe(false)
    expect(frame.props('slowLoad')).toBe(true)
    expect(frame.get('iframe').element).toBe(original)
    frame.vm.$emit('iframeLoad'); await nextTick()
    expect(frame.props('slowLoad')).toBe(false)
    expect(frame.props('loading')).toBe(false)
    expect(frame.get('iframe').element).toBe(original)
  })
  it('retains a loaded app and frame key through a transient readiness failure', async () => {
    const frame = render(); frame.vm.$emit('iframeLoad'); await nextTick()
    const original = frame.get('iframe').element, key = frame.props('refreshKey')
    readiness(false); await nextTick()
    expect(frame.props('uiReadyBlocked')).toBe(false)
    expect(frame.get('iframe').element).toBe(original)
    readiness(true); await nextTick()
    expect(frame.get('iframe').element).toBe(original)
    expect(frame.props('refreshKey')).toBe(key)
  })
  it('still gates a cold app, then starts it once readiness arrives', async () => {
    readiness(false); const frame = render()
    expect(frame.find('iframe').exists()).toBe(false)
    await vi.advanceTimersByTimeAsync(13000); await nextTick()
    expect(frame.props('slowLoad')).toBe(false)
    readiness(true); await nextTick()
    expect(frame.find('iframe').exists()).toBe(true)
    expect(frame.props('loading')).toBe(true)
  })
  it('an explicit refresh rechecks readiness instead of bypassing a cold app', async () => {
    const frame = render(); frame.vm.$emit('iframeLoad'); await nextTick()
    readiness(false); await nextTick()
    frame.vm.$emit('refresh'); await nextTick()
    expect(frame.props('uiReadyBlocked')).toBe(true)
    expect(frame.find('iframe').exists()).toBe(false)
  })
})
