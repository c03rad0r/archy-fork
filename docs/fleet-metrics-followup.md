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
