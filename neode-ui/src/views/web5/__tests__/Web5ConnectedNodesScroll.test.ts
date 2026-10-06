import { mount, enableAutoUnmount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Web5ConnectedNodes from '../Web5ConnectedNodes.vue'

// Structural pin for UIFIX-02: the connected-nodes card's height must track
// its row sibling (via grid `align-items: stretch` + a zero-basis flex
// child), and the tab panes must scroll inside that height rather than
// growing to fit every row. See:

enableAutoUnmount(afterEach)

vi.mock('vue-router', () => ({
  useRouter: () => ({ push: vi.fn() }),
}))

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

vi.mock('@/api/rpc-client', () => ({
  rpcClient: {
    listPeers: vi.fn(() => new Promise(() => {})),
    federationListNodes: vi.fn().mockResolvedValue({ nodes: [] }),
    federationListPendingRequests: vi.fn().mockResolvedValue({ requests: [] }),
    checkPeerReachable: vi.fn().mockResolvedValue({ reachable: false }),
    call: vi.fn(),
  },
}))

vi.mock('@/composables/useMessageToast', () => ({
  useMessageToast: () => ({
    loadReceivedMessages: vi.fn(),
  }),
}))

vi.mock('@/stores/web5Badge', () => ({
  useWeb5BadgeStore: () => ({ pendingRequestCount: 0 }),
}))

vi.mock('@/stores/app', () => ({
  useAppStore: () => ({ peerHealth: {} }),
}))

vi.mock('@/composables/useModalKeyboard', () => ({
  useModalKeyboard: vi.fn(),
}))

describe('Web5ConnectedNodes scroll contract (UIFIX-02)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    sessionStorage.clear()
  })

  it('gives all three tab panes the bounded, sibling-matched scroll contract', () => {
    const wrapper = mount(Web5ConnectedNodes)

    // The three v-show panes (trusted, observers, requests) are always in the
    // DOM regardless of which tab is active. The Send Message modal is
    // v-if="false" by default, so this selector matches exactly the panes.
    const panes = wrapper.findAll('.overflow-y-auto')
    expect(panes).toHaveLength(3)

    for (const pane of panes) {
      const classes = pane.classes()
      // Below the row breakpoint: unchanged stacked cap, still scrollable.
      expect(classes).toContain('min-h-0')
      expect(classes).toContain('overflow-y-auto')
      expect(classes).toContain('max-h-72')
      // At the row breakpoint: zero-basis growing flex child with no cap —
      // this is what lets the sibling drive the card's height while the
      // pane itself contributes nothing to intrinsic sizing.
      expect(classes).toContain('xl:flex-1')
      expect(classes).toContain('xl:basis-0')
      expect(classes).toContain('xl:max-h-none')
      // The old grow-to-fit-content class must be gone.
      expect(classes).not.toContain('flex-auto')
    }
  })

  it('gives the card root a min-h-0 flex column with a row-breakpoint height floor', () => {
    const wrapper = mount(Web5ConnectedNodes)

    const root = wrapper.find('[data-controller-container]')
    const classes = root.classes()

    expect(classes).toContain('flex')
    expect(classes).toContain('flex-col')
    expect(classes).toContain('min-h-0')
    expect(classes).toContain('xl:min-h-[40rem]')
  })

  it('still renders the empty-state row for each pane when the node list is empty', () => {
    const wrapper = mount(Web5ConnectedNodes)

    // Default state: no cached peers, observers, or connection requests.
    expect(wrapper.text()).toContain('web5.noPeers')
    expect(wrapper.text()).toContain('web5.noObservers')
    expect(wrapper.text()).toContain('web5.noRequests')

    // The panes themselves are still present in the tree (not collapsed away).
    expect(wrapper.findAll('.overflow-y-auto')).toHaveLength(3)
  })
})
