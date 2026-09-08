// Real vue-i18n instance (unlike ReceiveBitcoinModal.test.ts, which mocks
// `t` to a no-op and so cannot catch a bad message string). Operator report
// (2026-09-08): clicking the Ecash tab closed the whole Receive modal, in
// both the browser and the Android companion's WebView. Root cause: vue-i18n
// treats a bare `@` as the start of "linked message" syntax — `en.json`'s
// `receiveBitcoin.lnAddressLabel` ("Your @minibits.cash address:") isn't
// valid linked-message syntax, so *compiling* that message throws a
// SyntaxError the instant it's first rendered (i.e. the moment the address
// loads), and the uncaught render-function error blanks the whole teleported
// modal. Fixed by escaping it as `{'@'}` (the same pattern already used for
// `settings.domainNamePlaceholder`). This test uses the real compiler so a
// future bad interpolation string in this component fails fast in `npm test`
// instead of only in a live browser.
import { flushPromises, mount } from '@vue/test-utils'
import { describe, expect, it, vi } from 'vitest'
import ReceiveBitcoinModal from '../ReceiveBitcoinModal.vue'
import { rpcClient } from '@/api/rpc-client'
import i18n from '@/i18n'

vi.mock('@/api/rpc-client', () => ({
  rpcClient: { call: vi.fn() },
}))

vi.mock('@/composables/useLightningRequired', () => ({
  useLightningRequired: () => ({
    requireLightningReady: vi.fn().mockResolvedValue(true),
    handleLightningFailure: vi.fn().mockReturnValue(false),
  }),
}))

describe('ReceiveBitcoinModal — ecash tab with the real vue-i18n compiler', () => {
  it('renders the Minibits address label without an uncaught render error', async () => {
    vi.mocked(rpcClient.call).mockImplementation(async ({ method }: { method: string }) => {
      if (method === 'wallet.ecash-lnaddress') {
        return { address: 'someone@minibits.cash' } as never
      }
      return { claimed_count: 0, received_sats: 0, failed_count: 0 } as never
    })

    const wrapper = mount(ReceiveBitcoinModal, {
      props: { show: true },
      attachTo: document.body,
      global: { plugins: [i18n] },
    })
    let captured: unknown = null
    wrapper.vm.$.appContext.app.config.errorHandler = (err) => { captured = err }
    await flushPromises()

    const ecashTab = Array.from(document.body.querySelectorAll('button')).find((b) =>
      b.textContent?.toLowerCase().includes('ecash'),
    )
    expect(ecashTab).toBeTruthy()
    ecashTab!.dispatchEvent(new Event('click', { bubbles: true }))
    await flushPromises()
    await flushPromises()

    expect(captured).toBeNull()
    expect(wrapper.emitted('close')).toBeFalsy()
    const dialog = document.body.querySelector('[role="dialog"]')
    expect(dialog).toBeTruthy()
    expect(dialog?.textContent).toContain('minibits.cash')
    expect(dialog?.textContent).toContain('someone@minibits.cash')

    wrapper.unmount()
  })
})
