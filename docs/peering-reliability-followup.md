# Peer requests, delivery, and availability follow-up

Status: implementation under qualification. Not a claim of reciprocal live-node
acceptance or completion of the post-1.9 backlog.

## Confirmed failures

Yaya retained the dev node's approved inbound request while the dev node retained
its outbound Sent request, without reciprocal federation membership. Approval
previously had no durable delivery/retry record. Configured managed relays were
also omitted from reply publication; that separate repair is in 9a041bed.

The Connected Nodes card cached untimestamped reachability booleans, with cached
results overriding the shared store. A failing RPC was rendered as an offline
route. Nostr requests were absent from its Requests tab and badge.

## Changes

- Persist the approval decision and a node-key-encrypted reply before delivery.
  Retry the same invite with bounded exponential backoff, at most four eligible
  replies per background pass. Relay acknowledgement alone does not remove it;
  reciprocal membership does. Removed peers and expired requests are excluded.
- Recover legacy Approved rows through the same supported delivery path. Do not
  edit peer files by hand or elevate Observer relationships to Trusted.
- Serialize pending-store mutations and replace its private file atomically.
  Malformed storage is preserved and fails explicitly. Conflicting decisions
  cannot both win. Approved requests expire after 30 days to permit reconnect.
- Validate the invite's DID and key against the requested identity, normalize
  discovery trust to Observer before acceptance and callback.
- Poll in the background every 30 seconds, skipping missed ticks.
- Show Nostr requests in Connected Nodes with a direct link to review them.
  Preserve pending rows on failed refresh, and surface partial failures.
- Render independently arriving node lists, limit reachability probes to four,
  ignore superseded replies and age timestamped reachability after 90 seconds.
  An RPC failure is unknown; an explicit failed reachability check is unreachable.
  Report last successful contact without inventing continuous offline duration.
- Place online nodes first, then unknown and unreachable, preserving order within
  each group. Fleet describes stale reports as Not reporting. Map labels include
  last contact and dashed links indicate no recent contact, not a live route.

## Evidence so far

- Original full backend candidate: 1,693 passed, 4 ignored, no failures, through
  the isolated runner (`/tmp/archy-peering-full-backend.log`).
- Additional review added recovery-through-real-relay and retry-backoff checks;
  all 50 focused federation tests pass, including actual encrypted relay delivery
  for legacy Approved rows and suppression of duplicate attempts during backoff.
  Final formatted source also passes all 1,693 backend tests (4 ignored).
- Connected Nodes: 9 focused tests pass, including independent rendering,
  mixed failed/negative/successful probes, Nostr requests, cache age and clock skew.
- Fleet/request display: 13 focused tests pass.
- Type checking and production UI build pass. Final frontend suite: 1,238 tests
  in 153 files pass (`/tmp/archy-peering-full-ui-final.log`).
- Yaya Chromium at 390 and 1440px passes candidate-asset browser checks with
  deterministic peer RPC fixtures: availability text/order, approved Nostr requests,
  connection navigation and no page errors (`/tmp/archy-peering-browser-candidate-7.log`).
  Fixture checks are not evidence of actual reciprocal membership.
- Clean backend artifact at 10d31ae1: SHA256
  `120bd0f51fbceafeceb5557117a442fffc432a7b4d5115da1a163e472f7160da`.
  Actual-node deployment and reciprocal membership checks remain required.

The prior reply-rejection test expected a Pending row. This revision deliberately
supersedes that behavior: the decision remains Approved with delivery pending,
so a relay outage does not undo an operator decision or require reapproval.

## Read-only reciprocal membership recheck — 2026-10-08

Authenticated checks on the actual dev and Yaya nodes found exactly one entry
for each other's previously recorded DID. Both relationships remain Observer,
and each corresponding request remains Approved. Last contact was about34s
and3s old respectively at the snapshot. No invite, approval, trust change,
peer removal or explicit synchronization was issued. The check verified node
hostnames and used existing private sessions without printing credentials.

This closes the current persisted reciprocal-membership snapshot check, not the
wider outage/reconnect, new approval handshake, relationship-removal UI or FIPS
transport acceptance. Different unrelated peer counts are not treated as drift
caused by this read-only check. Evidence and replay script are archived under
`~/.local/state/archipelago/release-qualification/peering-readonly-20261008/`.
