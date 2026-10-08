import { describe, expect, it } from 'vitest'
import { usePeerPaymentOperations } from './peerPaymentOperations'

describe('payment operation completion ownership', () => {
  it('does not let a duplicated old completion clear the next operation in the same modal', () => {
    const ui = usePeerPaymentOperations(() => ({ peer: 'seller', item: 'file', generation: 1 }))
    const first = ui.begin('invoice', 'seller', 'file')!
    expect(ui.finish(first)).toBe(true)
    const second = ui.begin('invoice-status', 'seller', 'file')!
    expect(ui.selected(first)).toBe(false)
    expect(ui.finish(first)).toBe(false)
    expect(ui.current.value?.token).toBe(second.token)
    expect(ui.busy.value).toBe(true)
    expect(ui.finish(second)).toBe(true)
    expect(ui.busy.value).toBe(false)
  })
})
