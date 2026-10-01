# Angor client acceptance and remaining project recovery

Status: **OPEN — live services and one complete project flow pass; full project recovery is incomplete.**

The operator requires actual Angor-client verification and any necessary application
fixes to reach the next OTA, app catalog and ISO. Main release owner acknowledged
ownership of NPM/public management isolation; this investigation owns Angor evidence,
app setup documentation and the scoped browser test. Do not treat this report as
permission to waive the outstanding discovery/recovery requirements.

## Verified live behavior

- Trusted HTTPS indexer health, fees, address transactions and cursor pagination work.
- Real browser requests from `https://angor.io` reach the selected custom indexer;
  checked transaction IDs, confirmation data and output scripts match the reference.
- The indexer serves all 35 funding transactions listed in the public reference
  snapshot, confirmed, with matching original announcement commitments. This checks
  that snapshot, not every possible Bitcoin address or the entire project universe.
- Relay NIP-11 and encrypted WebSocket/Nostr read work. An invalid signature was
  rejected before importing the valid original announcement; the original signed
  announcement was accepted unchanged and read back exactly.
- Imported ONLY the original public kind3030 event
  `adbf94d6152097f3d503cd68cc00dee34c1e1007914c00cfc3d6698d1ee01834`, after checking its
  Schnorr signature with nostr-tools and matching funding transaction
  `72d14227b78a260c9410a65585cb49e81e35eaa95b1b23ec702eb0512ca016a7` on our indexer.
  No fabricated, re-signed or modified announcement, wallet operation or payment.
- Restarted only the managed Angor relay. Its original announcement persisted;
  node-internal strfry container ID and start time remained unchanged.
- Clean Chromium, configured with our indexer plus original relays and our relay,
  now displays **Casa Bitcoin sv** in Explore and loads its full project page,
  metadata, funding transaction and Project Statistics.
- `tests/lifecycle/angor-public-browser.cjs` passes all three acceptance groups:
  trusted public API/WSS and chain commitment; actual Explore discovery;
  actual project details/statistics. It is opt-in and read-only, uses a disposable
  profile and known immutable public fixture, and does not disable TLS checks.

Live test command (run from repository root):

```sh
ANGOR_TEST_INDEXER=https://angor-indexer.tx1138.com/ \
ANGOR_TEST_RELAY=wss://angor-relay.tx1138.com/ \
ANGOR_TEST_RELAYS='["wss://relay.angor.io","wss://relay2.angor.io","wss://angor-relay.tx1138.com"]' \
node tests/lifecycle/angor-public-browser.cjs
```

## Empty project list: reproduced upstream behavior

The user retained the original relays. Our earlier suggestion that an empty new
relay explained the entire failure was incorrect.

Angor Hub v2.0.0 (`main-575CWKJX.js`) was tested in separate clean Chromium profiles
using our indexer and `https://indexer.angor.io/`. Both received candidate Nostr
announcements, but the current batch failed validation: `event-mismatch` for
updated announcements of one funded project, and `not-found-in-mempool` for another
address that both indexers correctly return empty. Both displayed zero projects.
The default primary `fulcrum.angor.online` separately failed browser CORS in a
fresh default-config test. These upstream failures must not be attributed to
missing data in our indexer without evidence.

Current `block-core/angor-hub` `src/app/services/indexer.service.ts` discovers
kind3030 events, filters by network, and calls `validateAndAddProjects` in the
background. When a batch has candidates but none validate, it does not continue
discovery to older valid original announcements. The Load More UI is unavailable
when the project list is empty. An empty relay can coexist with this bug but is
not its sole cause. After the authentic announcement was copied to our relay,
one original passed the unchanged validation and appeared. This is a bounded
history-availability recovery, NOT an upstream UI fix or global relay sync.

The current browser uses ordinary `/api/v1/address/.../txs`, not the deprecated
`/api/v1/query/Angor/projects` listing. Our adapter's 404 for the latter is NOT the
observed browser failure. Do not add an opaque upstream proxy to hide that 404.

## Other projects remain unresolved

The public reference query returned 35 project records (limit50). All35 on-chain
funding commitments were verified through our indexer. Exact original-event-ID
queries were sent to the configured relays `relay.angor.io`, `relay2.angor.io`,
`nos.lol`, `relay.primal.net`, and `relay.damus.io` (Damus had an initial connection
failure, then answered EOSE on retry). Only one original announcement was found.
Additional queries to `relay.snort.social`, `nostr.mom`, and `no.str.cr` returned
no matches; `relay.nostr.band` and `relay.f7z.io` were unavailable. This does not
prove the other34 events are globally lost or that all storage sources were checked.

Exact project IDs, transaction IDs, event IDs and confirmed commitment results:
[recovery inventory](angor-project-recovery-20261001.json). Recover originals from
founders/known archives or authoritative relay backups and retain their signatures.
On-chain commitments cannot reconstruct missing off-chain project content.
Do not fabricate replacements or accept mismatched events to make cards appear.

## Release handoff and open gates

- [x] Release owner acknowledges these final findings and the 34-project gap.
      Confirmed 2026-10-01 18:46:50 UTC in `/tmp/angor-final-handoff-ack.txt`: full
      recovery remains open; publication held for required gates; preserve relay
      data and rerun browser acceptance after bridge migration and OTA.
- [ ] Resolve or explicitly carry the upstream discovery bug with accurate user
      guidance; do not claim full recovery because the fixture passes.
- [ ] Locate and verify remaining original project metadata/history, or obtain an
      explicit operator scope decision; one project is not complete acceptance.
- [ ] Preserve the verified public relay event/data through the candidate upgrade.
- [ ] Repeat scoped browser acceptance after NPM bridge migration and OTA install.
- [ ] Verify fresh ISO setup, NPM host propagation, public IP/default-host isolation,
      certificate issuance/renewal and relay WebSocket routing; see NPM handoff.
- [ ] Include updated app setup documentation and the read-only acceptance test.
      No Angor image/config change was justified by the verified transaction data;
      no image or catalog version should be bumped solely to disguise upstream failure.
- [ ] Existing signed transaction-broadcast integration tests remain required;
      this live investigation did not send real Bitcoin or test a funded investment.

Raw local diagnostic logs are in `/tmp/angor-*-browser*.log` and
`/tmp/angor-public-browser-acceptance.log`. The durable facts and recovery inventory
above must remain available after temporary logs expire.
