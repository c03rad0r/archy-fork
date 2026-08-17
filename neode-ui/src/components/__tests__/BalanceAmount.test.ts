import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import BalanceAmount from '../BalanceAmount.vue'
import HomeWalletCard from '@/views/home/HomeWalletCard.vue'
import i18n from '@/i18n'

/**
 * The distinction this whole component exists to protect: `0` is a balance,
 * `null` is the absence of one. Rendering the first when you mean the second
 * tells someone their money is gone, in the wallet's own typeface. Every case
 * below is really one assertion — that the two never get confused.
 */
describe('BalanceAmount', () => {
  it('shows the pixel readout when the balance is not known yet', () => {
    const w = mount(BalanceAmount, { props: { sats: null, label: 'on-chain balance' } })
    expect(w.find('.balance-pixels').exists()).toBe(true)
    expect(w.text()).not.toContain('0')
  })

  it('treats undefined the same as null', () => {
    // Optional props (`arkBalance?.spendable_sats`) arrive as undefined, not
    // null, and must not fall through to a figure.
    const w = mount(BalanceAmount, { props: { sats: undefined } })
    expect(w.find('.balance-pixels').exists()).toBe(true)
  })

  it('shows a genuine zero as a figure, not as loading', () => {
    // The inverse mistake: a node that really has no coins must be told so
    // plainly, not left shimmering forever.
    const w = mount(BalanceAmount, { props: { sats: 0 } })
    expect(w.find('.balance-pixels').exists()).toBe(false)
    expect(w.text()).toBe('0 sats')
  })

  it('formats a real balance with thousands separators', () => {
    const w = mount(BalanceAmount, { props: { sats: 9922 } })
    expect(w.text()).toBe('9,922 sats')
  })

  it('can drop the unit for bare figures', () => {
    const w = mount(BalanceAmount, { props: { sats: 21, suffix: '' } })
    expect(w.text()).toBe('21')
  })

  it('announces what is loading instead of being silently empty', () => {
    // A shimmering box with no text is nothing at all to a screen reader.
    const w = mount(BalanceAmount, { props: { sats: null, label: 'Cashu balance' } })
    const el = w.find('.balance-pixels')
    expect(el.attributes('role')).toBe('status')
    expect(el.attributes('aria-label')).toBe('Loading Cashu balance')
  })

  it('inherits the rail colour rather than hard-coding one', () => {
    // The pixels are painted with currentColor, which is what makes the
    // on-chain row orange and the Cashu row purple with no colour table to
    // keep in sync. Guard the mechanism: a literal colour here would drift.
    const w = mount(BalanceAmount, { props: { sats: null } })
    expect(w.find('.balance-pixel').exists()).toBe(true)
    expect(w.html()).not.toMatch(/background:\s*#|rgb\(/)
  })

  it('renders a 14x3 matrix scanned column by column', () => {
    // Column-first layout is what makes the lit column travel across as one
    // scan line; per-cell delays would make it crawl diagonally instead.
    const w = mount(BalanceAmount, { props: { sats: null } })
    const cells = w.findAll('.balance-pixel')
    expect(cells.length).toBe(42)
    // The three cells of a column share a delay; the next column steps on.
    const delay = (i: number) => cells[i]?.attributes('style') ?? ''
    expect(delay(0)).toBe(delay(1))
    expect(delay(1)).toBe(delay(2))
    expect(delay(3)).not.toBe(delay(2))
  })
})

describe('HomeWalletCard balances', () => {
  const base = {
    animate: false,
    walletConnected: true,
    walletOnchain: null,
    walletLightning: null,
    walletEcash: null,
    walletFedimint: null,
    walletArk: null,
    walletTransactions: [],
    isDev: false,
  }

  const mountCard = (props: Record<string, unknown>) =>
    mount(HomeWalletCard, { props: { ...base, ...props }, global: { plugins: [i18n] } })

  it('shows no figures at all before anything has loaded', () => {
    const w = mountCard({})
    // Six rows could be showing 0 sats here; none of them may.
    expect(w.findAll('.balance-pixels').length).toBeGreaterThan(0)
    expect(w.text()).not.toMatch(/\b0 sats\b/)
  })

  it('withholds the total until every rail it sums is known', () => {
    // A total computed with nulls as 0 would read *lower* than the rails
    // beneath it — worse than showing nothing, because it looks authoritative.
    const w = mountCard({ walletOnchain: 5000, walletLightning: null, walletEcash: 0, walletFedimint: 0 })
    expect(w.text()).not.toContain('5,000 sats\n')
    expect(w.findAll('.balance-pixels').length).toBeGreaterThan(0)
  })

  it('sums the total once every rail has reported', () => {
    const w = mountCard({ walletOnchain: 9000, walletLightning: 900, walletEcash: 22, walletFedimint: 0 })
    expect(w.text()).toContain('9,922 sats')
    expect(w.findAll('.balance-pixels').length).toBe(0)
  })

  it('shows an empty wallet as zero rather than as loading', () => {
    const w = mountCard({ walletOnchain: 0, walletLightning: 0, walletEcash: 0, walletFedimint: 0 })
    expect(w.findAll('.balance-pixels').length).toBe(0)
    expect(w.text()).toContain('0 sats')
  })

  it('keeps the Ark row hidden while its balance is unknown', () => {
    // Ark only appears once barkd reports something; "unknown" must not be
    // read as "> 0" and conjure a row on the many nodes with no Ark sidecar.
    const loaded = { walletOnchain: 1, walletLightning: 0, walletEcash: 0, walletFedimint: 0 }
    expect(mountCard({ ...loaded, walletArk: null }).text()).not.toContain('Ark')
    expect(mountCard({ ...loaded, walletArk: 0 }).text()).not.toContain('Ark')
    expect(mountCard({ ...loaded, walletArk: 7 }).text()).toContain('Ark')
  })
})
