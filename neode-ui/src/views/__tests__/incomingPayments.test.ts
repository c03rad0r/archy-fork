import { describe, it, expect } from 'vitest'
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

describe('incoming payments', () => {
  it('keeps an hours-old instant receipt in the badge until it is seen', async () => {
    // The regression: two hours past the old five-minute window. Nobody has
    // looked at it yet, so it is still news.
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
