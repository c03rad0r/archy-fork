# Node connection flow plan

Status: partial implementation prepared in an isolated worktree, not deployed.
Uses existing components, colors, spacing, glass cards, typography and motion.
Connection reliability and the remaining acceptance below must be qualified
before this flow ships.

## Implemented UI step — 2026-10-08

- Web5 exposes Connect with Nodes and Connected Nodes above the mobile collapse.
- Existing Federation route has Discover, Requests, Connected and Network Map
  tabs even with zero peers. The route query retains selection; legacy `view=list`
  opens Connected. Existing node detail modals preserve the underlying view.
- Discovery reuses the existing component inline, with local name/identity search,
  retained results/search across tab switches and stale-response rejection.
  The existing 30-second RPC timeout and signing confirmation remain in place.
  Presence advertisements explicitly do not imply reachability or authorization.
- Existing requests open Requests and existing peers open their detail; fresh
  requests still use the existing confirmation and Observer semantics. Own-node
  Trusted linking remains the separate, existing authenticated invite action.
- Requests has an empty state and active count. Action errors remain visible
  across tabs. Approved—connecting and Peer added remain distinct: persisted
  local membership does not assert reciprocal connection confirmation.
- Navigating away during initial identity loading cannot start a later orphaned
  polling timer. No backend, API contract, trust elevation or payment changes.

Validation: twelve focused tests across five files pass, plus app-project
`vue-tsc -p tsconfig.app.json --noEmit`. The initial combined focused run had
11 passes and one incorrect test assertion against a shallow component stub;
correcting the assertion to inspect its Vue prop yielded 2/2 targeted navigation
passes. No production source fix was needed for that assertion. Source browser
checks at 390/1440px are prepared but resource-queued behind IndeeHub; no visual,
production-artifact, actual companion or live-node acceptance is claimed.
Evidence: `release-qualification/connection-journey-20261008/` under the local
Archipelago state directory. Browser fixture allows only loopback synthetic RPC.

Still open: Cloud/Fleet return paths and explicit own-node entry, authenticated
reciprocal confirmation/retry acceptance, stale-advertisement age when supplied
by the protocol, desktop/phone UAT and before/after timings. This step does not
complete task 6 or satisfy the release gates below.

## Entry and return paths

- Web5 always exposes **Connect with Nodes**, including when mobile quick actions
  are collapsed. Keep **Connected Nodes** beside the entry or directly below it.
- Cloud peer files links to the same connection flow and retains its return
  location. Successful connection returns to that peer's files when appropriate.
- Fleet provides the same connection entry, with an explicit distinction between
  connecting to another person's node and linking a node the operator owns.
- Open the route immediately with cached safe summaries or a loading state;
  discovery and transport checks run after navigation. Do not await remote calls
  before rendering the destination. Cancel obsolete work on navigation away.

## Connect with Nodes

Use one page with existing tabs: **Discover**, **Requests**, **Connected**.
On mobile keep tabs in one horizontally scrollable row. Preserve the selected
view, search and scroll position when opening a node and returning.

Discover shows the existing opt-in Nostr presence results and an explicit invite
entry. Search updates locally; refresh provides immediate progress, timeout and
retry feedback. Distinguish stale advertisements from recently contacted nodes.
A presence event is discovery information, not authorization or proof of reachability.

Each node has a single clear action: Request connection, View request, or Open
node according to its actual state. Explain what information the request shares.
Avoid duplicate requests on repeated taps or when responses arrive late.

## Requests and approval

Display incoming and sent Nostr requests in the same Requests view, with counts
and a readable node identity/name. Incoming requests offer Approve or Reject;
sent requests offer Cancel. Keep completed history available but secondary.

An approval progresses through distinct states:

1. Request sent / Awaiting approval.
2. Approved / Connecting — authenticated invitation accepted, join not confirmed.
3. Connected — persisted relationship and authenticated reciprocal confirmation.
4. Connection delayed — show bounded retry and a useful error; retain the approved
   operation so restart, lost acknowledgement or transient outage can recover.

Do not label relay acceptance as peer connection. A retry must reuse the same
logical operation, prevent duplicate peers and retain the operator's trust choice.
Cancellation/rejection delivery failures must be visible rather than reported as
successfully notified. Define recovery for already-approved legacy requests.

Normal discovery connections grant Observer access. **Link your own nodes** must
be a separate explicit flow with existing ownership/authentication requirements;
being reachable over FIPS never grants Trusted access or remote management rights.

## Connected nodes and Fleet

Show actual connection state and last successful authenticated contact. Distinguish
**Offline**, **Connecting**, **Unknown** and **Metrics unavailable**. Last report
age alone does not establish when a node went offline. Future/skewed timestamps
must not make a node permanently online.

Default ordering: online, connecting, unknown, confirmed offline; stable ordering
within groups. Honor manually selected sorting/filtering and do not disrupt the
user's selection while metrics update. Offline rows show last contact; show an
"offline for" duration only when an observed transition supports it.

The existing network map uses matching status labels and accessible details;
color alone is insufficient. Node detail keeps Connect/Retry, Files and permitted
management actions together. Do not add duplicate connection mechanisms.

## Acceptance before deployment

- Two real nodes: request, approval, reciprocal connection and persisted lists.
- Retry after lost reply, duplicate/reordered events, restart on each side,
  unavailable relay, FIPS outage and supported transport recovery.
- Invalid signatures, unsolicited invites, wrong identities, stale/cancelled
  requests and blocked peers cannot gain access or elevate trust.
- Desktop and actual companion: first connection, revisit, back navigation,
  search, tab switching, refresh, background/resume and interrupted network.
- Measure tap-to-feedback, first usable content, discovery completion and
  approval-to-confirmed-connection before and after. Preserve unknown data.
- Operator UAT gives exact nodes, steps and expected states; no extra payment or
  wallet/channel changes are needed for connection testing.
