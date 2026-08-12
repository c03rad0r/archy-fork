import { describe, expect, it } from 'vitest'
import { explainReceiveAddressFailure } from '../bitcoinReceive'

describe('explainReceiveAddressFailure', () => {
  it('explains locked wallet failures', () => {
    expect(explainReceiveAddressFailure(new Error('wallet locked'))).toContain('wallet is locked')
  })

  it('explains sync failures', () => {
    expect(explainReceiveAddressFailure(new Error('chain backend is still syncing'))).toContain('still syncing')
  })

  it('explains empty address responses', () => {
    expect(explainReceiveAddressFailure(new Error('LND did not return a Bitcoin address'))).toContain('did not return an address')
  })

  it('explains lnd transport failures', () => {
    expect(explainReceiveAddressFailure(new Error('LND REST connection failed'))).toContain('not responding cleanly')
  })

  it('keeps bitcoin address generation failures visible', () => {
    expect(explainReceiveAddressFailure(new Error('Bitcoin address generation failed: LND is not ready'))).toContain('Bitcoin address generation failed')
  })

  describe('structured reason codes (backend [CODE] token)', () => {
    it('maps an unreachable REST endpoint to a starting/recovering message — NOT locked (.228 regression)', () => {
      const msg = explainReceiveAddressFailure(
        new Error('Bitcoin address unavailable [LND_REST_UNREACHABLE]: service not reachable'),
      )
      expect(msg).toContain('starting up or recovering')
      expect(msg.toLowerCase()).not.toContain('locked')
    })

    it('maps a genuinely locked wallet to the locked message', () => {
      expect(
        explainReceiveAddressFailure(new Error('Bitcoin address unavailable [LND_WALLET_LOCKED]: locked')),
      ).toContain('locked')
    })

    it('maps an uninitialized wallet to the setup message', () => {
      expect(
        explainReceiveAddressFailure(new Error('Bitcoin address unavailable [LND_WALLET_UNINITIALIZED]: x')),
      ).toContain('has not been set up')
    })

    it('maps a syncing wallet to the syncing message', () => {
      expect(
        explainReceiveAddressFailure(new Error('Bitcoin address unavailable [LND_SYNCING]: x')),
      ).toContain('syncing')
    })

    it('prefers the code over substrings even when the detail text is misleading', () => {
      // Detail mentions neither "locked" nor "unlock"; code must still win.
      const msg = explainReceiveAddressFailure(
        new Error('Bitcoin address unavailable [LND_REST_UNREACHABLE]: wallet service down'),
      )
      expect(msg).toContain('starting up or recovering')
    })
  })
})
