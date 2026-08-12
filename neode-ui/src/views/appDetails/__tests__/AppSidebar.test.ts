import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import AppSidebar from '../AppSidebar.vue'
import { PackageState } from '@/types/api'

function mountSidebar(manifestOverrides: Record<string, unknown>, propOverrides: Record<string, unknown> = {}) {
  const i18n = createI18n({
    legacy: false,
    locale: 'en',
    messages: {
      en: {
        appDetails: {
          access: 'Access',
          documentation: 'Documentation',
          gateway: 'Gateway',
          information: 'Information',
          lan: 'LAN',
          links: 'Links',
          ram: 'RAM',
          ramDesc: 'Memory required',
          requirements: 'Requirements',
          setupInstructions: 'Setup Instructions',
          requiresTor: 'Requires Tor',
          services: 'Services',
          sourceCode: 'Source Code',
          storage: 'Storage',
          storageDesc: 'Storage required',
          tor: 'Tor',
          website: 'Website',
        },
        common: {
          category: 'Category',
          developer: 'Developer',
          license: 'License',
          status: 'Status',
          version: 'Version',
        },
      },
    },
  })

  return mount(AppSidebar, {
    props: {
      pkg: {
        state: PackageState.Running,
        manifest: {
          id: 'example',
          title: 'Example',
          version: '1.0.0',
          license: '',
          ...manifestOverrides,
        },
        'static-files': { license: '', instructions: 'Follow step 1.\nThen step 2.', icon: '' },
      },
      packageKey: 'example',
      isWebOnly: false,
      gatewayState: 'stopped',
      interfaceAddresses: null,
      lanUrl: '',
      torUrl: '',
      showTorAddress: false,
      credentials: null,
      credentialsLoading: false,
      ...propOverrides,
    },
    global: {
      plugins: [i18n],
    },
  })
}

describe('AppSidebar', () => {
  it('renders manifest links with real URLs', () => {
    const wrapper = mountSidebar({
      website: 'https://example.com',
      'upstream-repo': 'https://github.com/example/app',
      'support-site': 'https://docs.example.com',
    })

    const hrefs = wrapper.findAll('a').map(anchor => anchor.attributes('href'))

    expect(hrefs).toEqual([
      'https://example.com',
      'https://github.com/example/app',
      'https://docs.example.com',
    ])
    expect(wrapper.text()).toContain('Website')
    expect(wrapper.text()).toContain('Source Code')
    expect(wrapper.text()).toContain('Documentation')
  })

  it('does not render dead placeholder links', () => {
    const wrapper = mountSidebar({
      website: '#',
      'upstream-repo': '',
      'support-site': undefined,
    })

    expect(wrapper.findAll('a')).toHaveLength(0)
    expect(wrapper.text()).not.toContain('Links')
  })

  it('shows a credentials loading state before credentials arrive', () => {
    const wrapper = mountSidebar({}, { credentialsLoading: true })

    expect(wrapper.text()).toContain('Credentials')
    expect(wrapper.text()).toContain('Loading credentials...')
  })

  it('renders setup instructions when provided', () => {
    const wrapper = mountSidebar({})

    expect(wrapper.text()).toContain('Setup Instructions')
    expect(wrapper.text()).toContain('Follow step 1.')
    expect(wrapper.text()).toContain('Then step 2.')
  })

  it('hides setup instructions when empty', () => {
    const wrapper = mountSidebar({}, {
      pkg: {
        state: PackageState.Running,
        manifest: {
          id: 'example',
          title: 'Example',
          version: '1.0.0',
          license: '',
        },
        'static-files': { license: '', instructions: '   ', icon: '' },
      },
    })

    expect(wrapper.text()).not.toContain('Setup Instructions')
  })
})
