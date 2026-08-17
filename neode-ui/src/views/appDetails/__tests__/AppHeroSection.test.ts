import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import AppHeroSection from '../AppHeroSection.vue'
import { PackageState, type PackageDataEntry } from '@/types/api'

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      common: {
        launch: 'Launch',
        restart: 'Restart',
        start: 'Start',
        stop: 'Stop',
        uninstall: 'Uninstall',
      },
      appDetails: {
        channels: 'Channels',
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
        long: 'Example app',
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

function mountHero(props: Partial<InstanceType<typeof AppHeroSection>['$props']> = {}) {
  return mount(AppHeroSection, {
    props: {
      pkg: packageData(),
      appId: 'example',
      packageKey: 'example',
      canLaunch: true,
      isWebOnly: false,
      pendingAction: null,
      ...props,
    },
    global: {
      plugins: [i18n],
    },
  })
}

describe('AppHeroSection', () => {
  it('disables app controls while a container action is running', () => {
    const wrapper = mountHero({ pendingAction: 'restart' })

    // A real ellipsis, not three dots — the label changed in 9ccc325a and
    // this assertion was left behind.
    expect(wrapper.text()).toContain('Restarting…')
    expect(wrapper.findAll('button').every(button => button.attributes('disabled') !== undefined)).toBe(true)
  })

  it('labels update progress and disables update controls', () => {
    const wrapper = mountHero({
      pendingAction: 'update',
      pkg: packageData({ 'available-update': '1.1.0' }),
    })

    expect(wrapper.text()).toContain('Updating...')
    const updateButtons = wrapper.findAll('button').filter(button => button.text().includes('Updating...'))
    expect(updateButtons.length).toBeGreaterThan(0)
    expect(updateButtons.every(button => button.attributes('disabled') !== undefined)).toBe(true)
  })
})
