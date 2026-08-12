import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import AppContentSection from '../AppContentSection.vue'
import { PackageState, type PackageDataEntry } from '@/types/api'

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      appDetails: {
        about: 'About {name}',
        features: 'Features',
        screenshots: 'Screenshots',
      },
    },
  },
})

function packageData(overrides: Partial<PackageDataEntry> = {}): PackageDataEntry {
  return {
    state: PackageState.Running,
    health: 'healthy',
    manifest: {
      id: 'example',
      title: 'Example',
      version: '1.0.0',
      description: {
        short: 'Example app',
        long: 'Example app details',
      },
      'release-notes': '',
      license: '',
      'wrapper-repo': '',
      'upstream-repo': '',
      'support-site': '',
      'marketing-site': '',
      'donation-url': null,
    },
    ...overrides,
  }
}

function mountContent(pkg: PackageDataEntry) {
  return mount(AppContentSection, {
    props: {
      pkg,
      features: [],
      needsBitcoinSync: false,
      bitcoinSynced: true,
      bitcoinSyncPercent: 100,
      bitcoinBlockHeight: 100,
    },
    global: {
      plugins: [i18n],
    },
  })
}

describe('AppContentSection', () => {
  it('does not show screenshot placeholders when no screenshots exist', () => {
    const wrapper = mountContent(packageData())

    expect(wrapper.text()).not.toContain('Screenshots')
    expect(wrapper.findAll('img')).toHaveLength(0)
  })

  it('renders real screenshot metadata when provided', () => {
    const wrapper = mountContent(packageData({
      manifest: {
        ...packageData().manifest,
        screenshots: [
          '/assets/screenshots/example-dashboard.png',
          { src: '/assets/screenshots/example-settings.png', alt: 'Settings screen' },
          '   ',
        ],
      },
    }))

    const images = wrapper.findAll('img')

    expect(wrapper.text()).toContain('Screenshots')
    expect(images).toHaveLength(2)
    expect(images[0]?.attributes('src')).toBe('/assets/screenshots/example-dashboard.png')
    expect(images[0]?.attributes('alt')).toBe('Example screenshot 1')
    expect(images[1]?.attributes('src')).toBe('/assets/screenshots/example-settings.png')
    expect(images[1]?.attributes('alt')).toBe('Settings screen')
  })
})
