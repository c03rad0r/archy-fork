import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from '@vue/test-utils'
import i18n from '@/i18n'
import HomeWalletCard from '@/views/home/HomeWalletCard.vue'

/**
 * Instant rails (Lightning, Cashu, Fedimint, Ark) settle immediately, so
 * there is no confirmation count to retire an incoming receipt from the
 * badge. It used to leave on a five-minute wall clock, which meant a payment
 * could arrive and vanish before anyone looked — and for ecash, which leaves
 * no public ledger entry, this panel was the only place the receipt was ever
 * shown.
 *
 * These cases pin the replacement: a receipt stays until it has been seen.
 */

const now = () => Math.floor(Date.now() / 1000)

function tx(over: Record<string, unknown> = {}) {
  return {
    tx_hash: '',
    amount_sats: 1000,
    direction: 'incoming' as const,
    num_confirmations: 1,
    time_stamp: now(),
    total_fees: 0,
    dest_addresses: [],
    label: '',
    block_height: 0,
    kind: 'cashu' as const,
    ...over,
  }
}

const base = {
  animate: false,
  walletConnected: true,
  walletOnchain: 0,
  walletLightning: 0,
  walletEcash: 0,
  walletFedimint: 0,
  walletArk: 0,
  isDev: false,
}

const mountCard = (transactions: ReturnType<typeof tx>[]) =>
  mount(HomeWalletCard, {
    props: { ...base, walletTransactions: transactions },
    global: { plugins: [i18n] },
  })

// Acknowledgement is stored per-browser, so each case starts from a clean
// slate — otherwise one test's "seen" set silently satisfies the next.
beforeEach(() => localStorage.clear())

describe('incoming payments', () => {
  it('retires a receipt on acknowledgement, never on age alone', async () => {
    // On a browser that has acknowledged before, an unacknowledged receipt
    // stays put however old it is — age is not the signal, being seen is.
    // (A brand-new browser is the separate first-run case below.)
    localStorage.setItem('archy-seen-incoming-v1', JSON.stringify(['some-earlier-receipt']))
    const w = mountCard([tx({ time_stamp: now() - 7200 })])
    expect(w.text()).toContain('Incoming 1')
  })

  it('does not clear the receipt merely because the panel was opened', async () => {
    // "Selecting incoming clears a pending token" — opening must show it,
    // not consume it. Someone reading the row must be able to keep reading.
    const w = mountCard([tx()])
    await w.find('button').trigger('click')
    expect(w.text()).toContain('Incoming Transactions')
    expect(w.text()).toContain('+1,000 sats')
  })

  it('marks it seen once the panel is closed again', async () => {
    localStorage.setItem('archy-seen-incoming-v1', JSON.stringify([]))
    const w = mountCard([tx()])
    const badge = w.find('button')
    await badge.trigger('click')   // open
    await badge.trigger('click')   // close — acknowledges
    expect(w.text()).not.toContain('Incoming 1')
  })

  it('still surfaces a payment that arrives after an earlier one was seen', async () => {
    const first = tx({ amount_sats: 1000, time_stamp: now() - 60 })
    const w = mountCard([first])
    const badge = w.find('button')
    await badge.trigger('click')
    await badge.trigger('click')
    expect(w.text()).not.toContain('Incoming 1')

    // A different payment must not inherit the first one's acknowledgement.
    await w.setProps({ walletTransactions: [first, tx({ amount_sats: 2500, time_stamp: now() })] })
    expect(w.text()).toContain('Incoming 1')
  })

  it('leaves on-chain transactions on their confirmation count', () => {
    // On-chain has a real signal and is deliberately untouched: it drops out
    // at three confirmations regardless of whether anyone looked.
    const unconfirmed = mountCard([tx({ kind: 'onchain', num_confirmations: 0, tx_hash: 'abc' })])
    expect(unconfirmed.text()).toContain('Incoming 1')

    const settled = mountCard([tx({ kind: 'onchain', num_confirmations: 6, tx_hash: 'abc' })])
    expect(settled.text()).not.toContain('Incoming 1')
  })

  it('lists several instant receipts separately even without txids', () => {
    // Instant rails carry no txid, so keying the list on tx_hash gave every
    // row the same empty key and Vue reused one node for all of them.
    const w = mountCard([
      tx({ amount_sats: 1000, time_stamp: now() - 10 }),
      tx({ amount_sats: 2000, time_stamp: now() - 20, kind: 'lightning' }),
      tx({ amount_sats: 3000, time_stamp: now() - 30, kind: 'fedimint' }),
    ])
    expect(w.text()).toContain('Incoming 3')
  })

  it('does not show a receipt again after a refresh', async () => {
    // The bug this whole model was supposed to prevent, and briefly caused:
    // "seen" lived in component state, so every page load forgot it and the
    // entire ecash history came back as new. Remounting is a refresh.
    const received = tx({ amount_sats: 4200, time_stamp: now() - 30 })
    const first = mountCard([received])
    const badge = first.find('button')
    await badge.trigger('click')
    await badge.trigger('click')
    expect(first.text()).not.toContain('Incoming 1')

    const afterRefresh = mountCard([received])
    expect(afterRefresh.text()).not.toContain('Incoming 1')
  })

  it('does not greet a brand-new browser with the whole history', () => {
    // Nothing acknowledged yet and a long history: treating all of it as
    // unseen would be the same wall of old receipts, just from the other
    // direction. Only what is genuinely recent counts as news on a first run.
    const old = [
      tx({ amount_sats: 100, time_stamp: now() - 86400 }),
      tx({ amount_sats: 200, time_stamp: now() - 3600 }),
      tx({ amount_sats: 300, time_stamp: now() - 600 }),
    ]
    expect(mountCard(old).text()).not.toContain('Incoming')

    // …but a receipt from moments ago still is.
    localStorage.clear()
    expect(mountCard([...old, tx({ amount_sats: 400, time_stamp: now() - 5 })]).text())
      .toContain('Incoming 1')
  })

  it('survives unreadable storage without resurrecting the history', () => {
    // A corrupt value must not read as "nothing has been acknowledged" — that
    // is precisely the refresh bug wearing a different hat.
    localStorage.setItem('archy-seen-incoming-v1', '{not json')
    const w = mountCard([tx({ amount_sats: 100, time_stamp: now() - 86400 })])
    expect(w.text()).not.toContain('Incoming')
  })

  it('does not silently turn into a navigation button while the panel is open', async () => {
    // The badge is two controls in one: with receipts it toggles the panel,
    // without them it navigates to the full transactions view. If the list
    // empties while the panel is open, the click under the user's cursor used
    // to change meaning and take them to another screen.
    const w = mountCard([tx()])
    const badge = w.find('button')
    await badge.trigger('click')
    expect(w.text()).toContain('Incoming Transactions')

    await w.setProps({ walletTransactions: [] })
    await badge.trigger('click')
    expect(w.emitted('showTransactions')).toBeUndefined()
  })
})
