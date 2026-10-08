import { beforeEach, describe, expect, it } from 'vitest'
import { keepCashuAttempt, parseCashuQuote, readCashuAttempt } from '../peerCashuPurchase'
const quote = { state: 'confirmation_required' as const, network: 'testnet' as const, mint_url: 'https://original.example.test/mint', operation_id: '12345678-1234-4234-8234-123456789abc', envelope_sha256: 'b'.repeat(64), gross_token_sats: 6, seller_net_sats: 5, wallet_debit_sats: 7, expires_at: 2_000_000_000 }
beforeEach(() => localStorage.clear())
describe('saved Cashu quote identity', () => {
  it('retains original network and mint across browser recovery and rejects substitution', () => {
    keepCashuAttempt('peer','file',quote,false)
    expect(readCashuAttempt('peer','file')?.quote).toEqual(quote)
    expect(() => keepCashuAttempt('peer','file',{...quote,network:'mainnet'},false)).toThrow('original purchase')
    expect(() => keepCashuAttempt('peer','file',{...quote,mint_url:'https://replacement.test'},false)).toThrow('original purchase')
    expect(readCashuAttempt('peer','file')?.quote).toEqual(quote)
  })
  it.each([{network:undefined},{network:'unknown'},{mint_url:undefined},{mint_url:'javascript:bad'},{mint_url:'https://user:secret@mint.test'}])('refuses an incomplete or unsafe identity %j', invalid => {
    expect(() => parseCashuQuote({...quote,...invalid})).toThrow('invalid payment quote')
  })
})
