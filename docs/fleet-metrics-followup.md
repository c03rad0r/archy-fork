# Fleet metric repair — qualification in progress

## Confirmed cause

The federation.get-state handler passed literal zero values for CPU, RAM, disk
and uptime into build_local_state. Peers therefore received a valid signed RPC
response containing fabricated measurements. Fleet also turned absent fields
into zeroes and treated a newly added peer's registration time as a report.
Read-only live inspection on the dev node found fifteen federation reports with
zero resource values, alongside two nonzero collector reports. Yaya has no
Trusted fleet reports; peer (Observer) access is deliberately distinct.

## Candidate change

- Use the existing minute MetricsStore sample, avoiding expensive per-peer probes.
- Samples older than three minutes or ahead of the local clock are unavailable.
- Read uptime from the host uptime source; errors remain unavailable.
- Preserve optional measurements through federation and the Fleet response.
- Display unavailable measurements separately from valid zeroes; average only
  valid measurements from reporting online nodes.
- Do not infer contact from when a peer was added. Invalid/future report dates
  show unknown status; offline cards state when the last report arrived.
- Keep existing trust restrictions and signed FIPS-preferred federation transport.
  This change does not claim FIPS media-stream acceptance or fix stalled peering.

## Qualification

All 1,684 backend tests pass (four ignored), including actual-handler fresh,
absent, stale and future sample coverage. All 11 Fleet helper/component tests
pass, including real-zero versus missing values, malformed/future dates,
ordering, averages and rendered unavailable/offline states. Frontend typecheck
passes. The first component assertion incorrectly expected spaces between
separate elements; it was corrected to inspect those elements. No deployment
or live metric repair acceptance yet.

Required: actual-handler fresh/absent/stale/future samples; full relevant suites;
mobile/desktop browser layout; upgraded sender and receiver comparing local
Monitoring to received Fleet values; restart/reconnect, offline ageing and real
transport evidence. Legacy senders still advertising zeros need the sender fix;
the receiver cannot reliably distinguish a legacy fabricated zero from idle CPU.

## Dev deployment and collector follow-up

Dev qualification deployment uses backend041f1fa2 (SHA256
11b62f697a71b762bf8638063d1a858a68eea6d7268e300f6320c99068efa78a)
and frontend3d0c67eb. Health and unchanged app-container identities/start times
pass. Live federation CPU/memory/disk values exactly match local Monitoring;
Web5 mobile390px/desktop1440px checks pass. Rollback retained on the node at
/var/lib/archipelago/support/followup-20261005-2120. Yaya remains on the prior
backend; no receiver/fleet-wide acceptance is claimed.

Live testing caught an unbounded podman stats subprocess delaying the first
snapshot, and a300-second collection interval conflicting with180-second Fleet
freshness. The next candidate starts after5seconds and collects once per minute,
with3-second df and8-second podman deadlines and kill-on-drop cleanup. Failed
system reads no longer become invented zero samples. Container-stat failure
leaves container readings unavailable while retaining valid system readings.
The actual subprocess timeout/reaping regression passes; all1,686 backend tests
pass (four ignored). This collector correction is not deployed yet.

Framework SSH and backend health work, but its stored dashboard session returns
401. Authenticated Framework Monitoring acceptance remains open. No authentication
boundary was bypassed to produce an apparent pass.
