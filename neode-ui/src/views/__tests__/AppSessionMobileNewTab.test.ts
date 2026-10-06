import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import AppSession from '../AppSession.vue'

const { mockReplace, mockPush, mockWindowOpen, mockSuppress, mockResume } = vi.hoisted(() => ({
  mockReplace: vi.fn(() => Promise.resolve()),
  mockPush: vi.fn(() => Promise.resolve()),
  mockWindowOpen: vi.fn(),
  mockSuppress: vi.fn(),
  mockResume: vi.fn(),
}))

vi.mock('vue-router', () => ({
  useRoute: () => ({
    params: { appId: 'gitea' },
    query: { returnTo: '/dashboard/apps' },
    fullPath: '/dashboard/apps/session/gitea',
  }),
  useRouter: () => ({ replace: mockReplace, push: mockPush }),
}))

vi.mock('@/stores/appLauncher', () => ({
  useAppLauncherStore: () => ({ panelAppId: null }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ data: { 'package-data': {} } }),
}))

vi.mock('@/stores/screensaver', () => ({
  useScreensaverStore: () => ({ suppress: mockSuppress, resume: mockResume }),
}))

vi.mock('../appSession/useAppIdentity', () => ({
  useAppIdentity: () => ({
    onIdentitySelected: vi.fn(),
    onIframeLoadIdentity: vi.fn(),
    handleIdentityRequest: vi.fn(),
    getStoredIdentity: vi.fn(),
  }),
}))

vi.mock('../appSession/useNostrBridge', () => ({
  useNostrBridge: () => ({
    handleNostrRequest: vi.fn(),
    showConsent: { value: false },
    consentRequest: { value: null },
    consentPhase: { value: 'review' },
    consentError: { value: '' },
    approveConsent: vi.fn(),
    denyConsent: vi.fn(),
    cancelPending: vi.fn(),
    dispose: vi.fn(),
  }),
}))

vi.stubGlobal('open', mockWindowOpen)

describe('AppSession mobile new-tab apps', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
    Object.defineProperty(window, 'innerWidth', {
      value: 390,
      writable: true,
      configurable: true,
    })
    Object.defineProperty(window, 'location', {
      value: { hostname: '192.0.2.10' },
      writable: true,
      configurable: true,
    })
  })

  it('opens tab-only apps directly on mobile instead of showing an interstitial', async () => {
    const wrapper = mount(AppSession, {
      global: {
        stubs: {
          Teleport: true,
          AppSessionHeader: true,
          NostrIdentityPicker: true,
          MobileGamepad: true,
        },
      },
    })
    await flushPromises()

    // Tab-only app (gitea) on mobile-web: open directly in a new browser tab
    // (no native bridge in the test) and dismiss the empty session — no
    // "this app opens in a tab" interstitial.
    expect(mockWindowOpen).toHaveBeenCalled()
    expect(mockReplace).toHaveBeenCalled()
    expect(wrapper.text()).not.toContain('This app opens in a new tab')
  })
})
