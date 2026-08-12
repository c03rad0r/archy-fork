import { describe, expect, it } from 'vitest'
import { mount } from '@vue/test-utils'
import { createI18n } from 'vue-i18n'
import MarketplaceAppCard from '../MarketplaceAppCard.vue'
import type { MarketplaceApp } from '../marketplaceData'

const app: MarketplaceApp = {
  id: 'btcpay-server',
  title: 'BTCPay Server',
  version: '2.0.6',
  description: 'Self-hosted Bitcoin payments',
  icon: '',
  dockerImage: 'btcpayserver/btcpayserver:2.0.6',
  category: 'commerce',
  source: 'community',
}

function mountCard(installed: boolean, installBlockedReason?: string) {
  const i18n = createI18n({
    legacy: false,
    locale: 'en',
    messages: {
      en: {
        common: { install: 'Install', launch: 'Launch', notAvailable: 'Not available' },
        marketplace: { alreadyInstalled: 'Already installed' },
      },
    },
  })

  return mount(MarketplaceAppCard, {
    props: {
      app,
      index: 0,
      stagger: false,
      installed,
      installing: false,
      installProgress: undefined,
      installedState: installed ? 'running' : null,
      startingUp: false,
      containersScanned: true,
      tierLabel: 'recommended',
      installBlockedReason,
    },
    global: {
      plugins: [i18n],
    },
  })
}

describe('MarketplaceAppCard', () => {
  it('shows tier badges before install', () => {
    const wrapper = mountCard(false)
    expect(wrapper.find('.tier-badge').exists()).toBe(true)
    expect(wrapper.text()).toContain('recommended')
  })

  it('hides tier badges after install', () => {
    const wrapper = mountCard(true)
    expect(wrapper.find('.tier-badge').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('recommended')
  })

  it('explains archive-node-only install blocks on cards', () => {
    const wrapper = mountCard(false, 'You need a full archival bitcoin node before downloading ElectrumX')
    expect(wrapper.text()).toContain('Requires a full archive Bitcoin node before install.')
    expect(wrapper.text()).toContain('Bitcoin Pruned')
  })
})
