import { mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useAppStore } from '@/stores/app'
import { PackageState, type DataModel, type PackageDataEntry } from '@/types/api'
import HealthNotifications from '../HealthNotifications.vue'

function makePkg(id: string, state: PackageState = PackageState.Running, health: string | null = 'healthy'): PackageDataEntry {
  return {
    state,
    health,
    manifest: {
      id,
      title: id,
      version: '1.0.0',
      description: { short: '', long: '' },
      'release-notes': '',
      license: '',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': '',
      'donation-url': null,
      interfaces: { main: { ui: true } },
    } as unknown as PackageDataEntry['manifest'],
  }
}

function makeData(pkg?: PackageDataEntry, timestamp = new Date().toISOString()): DataModel {
  return {
    'server-info': {
      id: 'node',
      version: '1.0.0',
      name: null,
      pubkey: '',
      'status-info': {
        restarting: false,
        'shutting-down': false,
        updated: false,
        'backup-progress': null,
        'update-progress': null,
      },
      'lan-address': null,
      'tor-address': null,
      unread: 0,
      'wifi-ssids': [],
      'zram-enabled': false,
      'seed-backed': false,
    },
    'package-data': pkg ? { indeedhub: pkg } : {},
    notifications: [{
      id: 'health-1',
      level: 'error',
      title: 'indeedhub is unhealthy',
      message: 'indeedhub health check failed',
      timestamp,
      app_id: 'indeedhub',
    }],
    ui: {
      name: null,
      'ack-welcome': '',
      marketplace: {
        'selected-hosts': [],
        'known-hosts': {},
      },
      theme: 'dark',
    },
  }
}

describe('HealthNotifications', () => {
  let pinia: ReturnType<typeof createPinia>

  beforeEach(() => {
    pinia = createPinia()
    setActivePinia(pinia)
    vi.useRealTimers()
  })

  it('shows active unhealthy package notifications', () => {
    const store = useAppStore(pinia)
    store.data = makeData(makePkg('indeedhub', PackageState.Running, 'unhealthy'))

    const wrapper = mount(HealthNotifications, {
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).toContain('indeedhub is unhealthy')
  })

  it('hides stale package notifications once health recovers', () => {
    const store = useAppStore(pinia)
    store.data = makeData(makePkg('indeedhub', PackageState.Running, 'healthy'))

    const wrapper = mount(HealthNotifications, {
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).not.toContain('indeedhub is unhealthy')
  })

  it('hides package notifications while an app is being removed', () => {
    const store = useAppStore(pinia)
    store.data = makeData(makePkg('indeedhub', PackageState.Removing, 'unhealthy'))

    const wrapper = mount(HealthNotifications, {
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).not.toContain('indeedhub is unhealthy')
  })

  it('hides old package health notifications on reload even if the app is still unhealthy', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-10T12:00:00Z'))

    const store = useAppStore(pinia)
    store.data = makeData(
      makePkg('indeedhub', PackageState.Running, 'unhealthy'),
      new Date('2026-06-10T11:20:00Z').toISOString(),
    )

    const wrapper = mount(HealthNotifications, {
      global: {
        plugins: [pinia],
      },
    })

    expect(wrapper.text()).not.toContain('indeedhub is unhealthy')
  })
})
