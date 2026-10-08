# Synthetic paid recovery across process restart — 2026-10-08

Status: **isolated qualification passed; actual authenticated FIPS and live-node response-loss acceptance remain open.** No real funds or live services were changed.

Reviewed test commits `62d19d8d` and `9eb99e47` were integrated as `1684d790` and `06a92f1f`. A single combined suite qualified candidate `1dbca844`, including the latest embedded IndeeHub helper `1a409900`, with **2,028 tests passed, zero failed and five ignored**. All **536 tracked backend/helper/catalog input hashes** remained unchanged. Compilation took 11m00s; execution used `scripts/test-backend-isolated.sh`, one compiler job, offline dependencies and the existing shared target cache. Tests ran in private network, filesystem and process namespaces.

The new process fixture runs disposable buyer and seller child processes over loopback HTTP with a fake Cashu mint. It deliberately closes the settlement response before HTTP headers after the seller has committed its receipt and redemption. The original buyer exits without recording delivery. Both buyer and seller are restarted, then recovery proves:

- The original purchase operation is reused without another consent/payment.
- Exactly one fake-mint redemption occurred, with buyer balance zero and seller balance eight synthetic sats, one transaction each.
- The recovered capability authorizes delivery; the production verified stream and owned-file storage recover the exact original bytes.
- Child logs are bounded, requests and child waits have deadlines, and cleanup cannot affect host services.

The full suite also retained all eleven prior payment/recovery checks, both rental cross-rail admission cases, Fleet provenance checks and managed-update recovery/ownership cases. The child entrypoint returns without doing work during ordinary suite enumeration; its parent test explicitly launches it with the disposable fixture context.

This does **not** qualify actual FIPS identity/transport, two live nodes, real mint payouts or a new payment. The separate release paid-file incident remains open. Existing cached-download live checks remain their own evidence. No source/artifact deployment or publication follows solely from this pass.

Evidence directory:
`/home/archipelago/.local/state/archipelago/release-qualification/paid-process-recovery-20261008/`

- `receipt.json`: `9c01abb83e2bb2d0e4cf9d5790578ba446de9a7b91609540e068858262c48247`
- `inputs.json`: `a5526b7493ed6c17814c2d8d407a7b61458db917341a6dd9fac42ff0554fd34b`
- `backend.log`: `62db27e6c0ddf2275ccb0f43ac663bac8b94184cf61800fb4755c7e8e6e31edd`
- `run.sh`: `e456fc7d13accb5bcf3671d20bd135fb1e349dca958e289f989813c7ccf5c974`

The compiler slot was handed directly to IndeeHub for its matching normal executable and actual update/rollback qualification. No backend/helper source changes were made after the captured input manifest.
