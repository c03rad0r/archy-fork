// Focused regression for the 2026-10-10 disk display change: the Home System
// card must show FREE capacity (live, post-cleanup accurate) rather than the
// used/total pair, which kept rendering the pre-cleanup volume after files
// were removed. Also covers TB-scale formatting.

import { mount, RouterLinkStub } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import HomeSystemCard from '../HomeSystemCard.vue'

vi.mock('vue-i18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}))

const TB = 1024 ** 4
const GB = 1024 ** 3

function stats(overrides: Record<string, unknown> = {}) {
  return {
    cpuPercent: 10,
    memUsed: 100,
    memTotal: 200,
    memPercent: 50,
    diskUsed: 512 * GB,
    diskTotal: 2 * TB,
    diskPercent: 25,
    bitcoinSyncPercent: 100,
    bitcoinBlockHeight: 800000,
    bitcoinAvailable: false,
    ...overrides,
  }
}

describe('HomeSystemCard disk display', () => {
  it('shows free capacity with TB units instead of used/total', () => {
    const wrapper = mount(HomeSystemCard, {
      props: { animate: false, loaded: true, stats: stats(), uptimeDisplay: 'up 1 day' },
      global: { stubs: { RouterLink: RouterLinkStub } },
    })
    const text = wrapper.text()
    expect(text).toContain('1.5 TB free / 2.0 TB')
    expect(text).not.toContain('512 GB / 2.0 TB')
    wrapper.unmount()
  })

  it('never renders a negative free value when usage exceeds the total', () => {
    const wrapper = mount(HomeSystemCard, {
      props: {
        animate: false,
        loaded: true,
        stats: stats({ diskUsed: 3 * TB, diskTotal: 2 * TB, diskPercent: 150 }),
        uptimeDisplay: 'up 1 day',
      },
      global: { stubs: { RouterLink: RouterLinkStub } },
    })
    expect(wrapper.text()).toContain('0 B free / 2.0 TB')
    wrapper.unmount()
  })
})
