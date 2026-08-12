/**
 * PUBLIC-DEMO-ONLY IndeeHub sign-in seeder.
 *
 * Served at /__demo/indee-demo-signin.js on the :2101 IndeeHub whole-origin
 * demo proxy (see nginx-demo.conf) and injected into the proxied site's HTML
 * <head> via sub_filter. It only ever runs on the demo's :2101 origin inside
 * the demo iframe — it never ships in real-node artifacts.
 *
 * THROWAWAY DEMO IDENTITY — NOT A SECRET. The embedded secp256k1 keypair was
 * freshly generated for the public demo (2026-07-29) and has never belonged
 * to any real user. Its whole purpose is to be a shared, public "demo
 * visitor" Nostr identity so the embedded IndeeHub boots signed in with no
 * login wall. Anyone extracting this key can only impersonate the demo
 * visitor, by design (threat T-gjd-01: accepted).
 *
 * How it works: IndeeHub's bundle (applesauce-accounts) restores accounts on
 * boot from localStorage key "indeedhub-accounts" (JSON array of serialized
 * accounts; a "nsec" private-key account deserializes as
 * { id, type: "nsec", pubkey, metadata, signer: { key: <hex sk> } }) and
 * activates the account whose id is stored under "indeedhub-active-account".
 * This classic script executes before the SPA's deferred module bundle, so
 * seeding here is visible to that boot-restore. Seeding is idempotent: an
 * existing non-empty account list is never overwritten.
 */
;(function () {
  'use strict'

  var ACCOUNTS_KEY = 'indeedhub-accounts'
  var ACTIVE_KEY = 'indeedhub-active-account'

  // Throwaway demo keypair (see header — public by design, not a secret).
  var DEMO_SK_HEX = 'ce2ffa96f99968beffc789cbba5d8b52f4a3020454dcaf77c2b553961bf5a8c9'
  var DEMO_PK_HEX = '7261540160244ec65ce0bf86ba03997e9b1b3b35c277e416bf1c7ba4271fee31'
  var DEMO_ACCOUNT_ID = 'archy-demo-visitor'

  try {
    var existing = null
    try {
      existing = JSON.parse(localStorage.getItem(ACCOUNTS_KEY))
    } catch (e) {
      existing = null
    }
    if (Array.isArray(existing) && existing.length > 0) return

    var account = {
      id: DEMO_ACCOUNT_ID,
      type: 'nsec',
      pubkey: DEMO_PK_HEX,
      metadata: { name: 'Archy Demo' },
      signer: { key: DEMO_SK_HEX },
    }
    localStorage.setItem(ACCOUNTS_KEY, JSON.stringify([account]))
    localStorage.setItem(ACTIVE_KEY, DEMO_ACCOUNT_ID)
  } catch (e) {
    // localStorage unavailable (e.g. blocked third-party storage) — the demo
    // visitor just sees IndeeHub's normal signed-out state.
  }
})()
