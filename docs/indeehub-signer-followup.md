# IndeeHub native signer follow-up

## Reproduced on Yaya

The installed app and dashboard have the same provider SHA256
`529fe82e7c16b51c62678427f565048c3dd93ca28320bd8494c17236ff57bb81`.
A clean mobile Chromium session opens the identity picker. After selecting the
existing profile and Authenticate, the picker disappears but the broker stays
full-screen (`aria-hidden=false`, 390 by 844). Its document has no visible text
or buttons, and the app still shows Sign In. The trace contains signer-ready and
signer-show but no signer-identity or signer-hide through 18 seconds.

The picker emits an entry from a Vue reactive array. NostrTabSigner passes that
proxy object directly to cross-frame postMessage after hiding the picker.
Structured cloning rejects reactive proxies, interrupting the handoff before
it schedules the broker hide. Reload uses the already-serialized identity from
localStorage, explaining why refresh can appear to repair the problem.

## Candidate fix

Send an explicit plain object containing only the selected public identity
fields. The private signer remains on the node; unrelated metadata is excluded.
Consent behavior and the existing green completion animation are unchanged.

A regression uses a real Vue reactive identity and structuredClone, verifies
that the proxy itself is rejected, the public handoff is cloneable, metadata is
excluded, and the overlay closes. Eleven focused signer/provider tests pass,
and frontend typecheck passes. A browser qualification using candidate dashboard
assets with the actual Yaya app/auth backend is in progress. No deployment or
actual companion acceptance is claimed yet.

## App source and further review

The canonical app is the Vite/Vue source in the separate IndeeHub repository,
not the obsolete Next.js Dockerfile under apps/indeedhub. Its existing local
nginx edit and untracked workflow documentation were preserved; new app work
uses a separate worktree.

Review found additional app-side concerns to test: production network failures
can flip the app into mock mode and fabricate subscribed users, and the header
can discard a valid backend Nostr session when the local signer account has not
been restored. Do not claim these repaired by the broker object-cloning fix.

## Live candidate qualification and restored-session prompting

2026-10-05: actual Yaya NIP-98 session exchange returns201 and authenticated
profile returns200 using candidate dashboard and app assets. The overlay hides;
a full refresh reuses the session and profile without another auth exchange.
Evidence: /tmp/archy-indeehub-full-candidate-2.log. This uses headless Chromium
390x844, real cookies/signatures/RPC/API, with only static candidate assets routed
locally. Initial asset-routing attempts hit Chromium private-network checks;
forwarding real API requests in the fixture resolved that test harness failure.
No backend authentication was mocked or bypassed.

Public-key lookups can inherit transient activation from the identity-picker
click/reload. The provider now avoids interpreting a restored-session hint or
already selected identity as a new account-switch request. Requests still pass
to the authenticated broker; the hint grants no signature permission. Explicit
selectIdentity remains available. Twelve provider/tab-signer regressions pass.

The combined frontend production check found strict TypeScript nullability
errors in Fleet test array indexing; the fixture now asserts both cards exist
and uses non-null indexing. No production Fleet behavior changed in that repair.
Actual deployment, companion lifecycle and the remaining recovery cases stay open.
