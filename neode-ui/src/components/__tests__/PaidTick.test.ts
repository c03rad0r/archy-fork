import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import i18n from '@/i18n'
import ScreensaverRing from '../ScreensaverRing.vue'
import SendBitcoinModal from '../SendBitcoinModal.vue'
import WalletScanModal from '../WalletScanModal.vue'

// Both modals fetch balances/fees on open. None of that is what this suite is
// about — stub the transport so mounting is deterministic and offline.
vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    call: vi.fn().mockResolvedValue({}),
  },
}))

// WalletScanModal reaches for camera/QR APIs jsdom does not implement.
beforeEach(() => {
  if (!navigator.mediaDevices) {
    Object.defineProperty(navigator, 'mediaDevices', {
      value: { getUserMedia: vi.fn().mockRejectedValue(new Error('no camera')), enumerateDevices: vi.fn().mockResolvedValue([]) },
      configurable: true,
    })
  }
})

// Teleported modal markup outlives the wrapper's root, so clear it between
// cases — otherwise one modal's nodes answer the next one's queries.
afterEach(() => {
  document.body.innerHTML = ''
})

const mountOpts = { props: { show: true }, global: { plugins: [i18n] } }

describe('paid tick renders the branded ring (FED-06)', () => {
  it('SendBitcoinModal: payment success shows exactly one badge ring, no ripple burst', async () => {
    const wrapper = mount(SendBitcoinModal, mountOpts)
    // Drive the component into its settled-payment state directly — this
    // suite is about what success *renders*, not how a payment settles.
    ;(wrapper.vm as unknown as Record<string, unknown>).successInfo = {
      amount: 12345,
      methodLabel: 'Sent via Lightning',
    }
    await flushPromises()

    const rings = wrapper.findAllComponents(ScreensaverRing)
    expect(rings).toHaveLength(1)
    expect(rings[0]?.props('size')).toBe('badge')

    // BaseModal teleports its content to <body>, so the rendered markup lives
    // outside the wrapper's own root element — assert against the document.
    // The checkmark core survives the ring swap...
    expect(document.querySelector('.burst-core')).not.toBeNull()
    expect(document.querySelector('.burst-check')).not.toBeNull()
    // ...and the CSS ripple elements it replaced are gone entirely.
    expect(document.querySelectorAll('.burst-ring')).toHaveLength(0)

    // Decoration only: the amount and SENT copy are untouched.
    expect(document.body.textContent).toContain('12,345')
    expect(document.body.textContent).toContain('SENT')

    wrapper.unmount()
  })

  it('WalletScanModal: success pane shows the same badge ring and keeps its checkmark', async () => {
    const wrapper = mount(WalletScanModal, mountOpts)
    ;(wrapper.vm as unknown as Record<string, unknown>).pane = 'success'
    await flushPromises()

    const rings = wrapper.findAllComponents(ScreensaverRing)
    expect(rings).toHaveLength(1)
    expect(rings[0]?.props('size')).toBe('badge')
    expect(document.querySelector('.burst-core')).not.toBeNull()
    expect(document.querySelector('.burst-check')).not.toBeNull()
    // The plain fixed-size circle the ring replaced is gone.
    expect(document.querySelectorAll('.success-ring')).toHaveLength(0)

    wrapper.unmount()
  })
})
