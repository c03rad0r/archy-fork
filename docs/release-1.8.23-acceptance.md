# Archipelago 1.8.23-alpha acceptance

Status: PREPARING. Do not publish until artifact checks and offline signatures pass.

## Scope

Durable Lightning paid-file entitlements and delivery retries, atomic buyer
ownership, compact persistent upload progress/cancellation, mobile transaction
filters, Immich inventory and retired-app uninstall, Portainer duplicate-network
migration, channel-close fee selection, and shared app-search clearing.

Angor Indexer and the optional Angor Relay remain in the signed catalog. They
were already included in 1.8.22; full-chain acceptance still awaits dev Bitcoin
initial sync. Do not advertise full-chain verification as complete.

## Validation carried into preparation

- Candidate deployed on dev and Framework with matching backend/dashboard bytes.
- Native Bitcoin/LND and Framework Immich container IDs/start times preserved.
- Six live desktop/mobile app-search cases passed.
- Actual uploads verified exact bytes, original destination after navigation,
  compact progress geometry and cancellation. These are browser checks, not
  physical companion acceptance.
- Framework seller's settled invoice recovered to a mode-0600 entitlement and
  remained paid across another manager restart; mismatched item rejected.
- Framework CryptPad stale inventory removed with data preserved; removal
  persisted after management restart. Dev normal package.uninstall RPC also
  completed with preserve_data=true and zero cleanup errors for the retired ID.
- Cooperative-close fee selector tested with intercepted requests; no real
  channel was closed. Real payment recovery never sent another payment.
- Shorty's Mempool inspected read-only; frontend/API healthy for over two weeks,
  systemd zero restarts. Operator reported normal status after their update.

## Follow-ups accepted for release preparation

The operator authorized release preparation after available tests pass.

- Actual failed-purchase delivery still requires buyer/file confirmation. The
  located invoice's source file is missing from Framework's recorded paths.
  Seller settlement recovery does not prove buyer delivery.
- Physical companion upload diagnosis needs the affected device/route.
- Framework rendered inventory/uninstall checks need dashboard second-factor
  authentication; SSH/runtime and dev API acceptance are recorded separately.
- Angor full-chain indexing awaits Bitcoin IBD.
- Prior Primal automatic-comment and lost-response ecash receipt follow-ups
  retain their documented boundaries; this release does not claim to fix them.

## Final release checks

Pending full release gates, versioned builds, exact artifact deployment, ISO
payload/boot acceptance, pinned-root signatures and publication verification.
No universal or future-failure guarantee is implied by these tests.

## Required NPM certificate gate

The release owner acknowledged `docs/npm-certificate-handoff-20261001.md` in
`/tmp/npm-release-handoff-ack.txt`. Shorty's npm-8 issuance and HTTPS health 200
were reported by the operator; durable data-path resolution, safe host routing,
automatic certificate renewal/reload, and the handoff acceptance matrix remain
required before publication. Earlier authorization does not waive this new gate.
