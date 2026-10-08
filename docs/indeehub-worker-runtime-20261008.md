# IndeeHub worker image runtime qualification — 2026-10-08

Status: isolated actual-image shutdown qualification passed; live delivery remains open.

Source: `29627fc3a316316e5f75fe799218ff41a2a0dea8`.
Image: `9b3ef4116d4b998820e25e09eda7ca14f23e9e264158629451b81e32b47d9c8e`.
OCI archive SHA256: `6db29188c0e68ed8e9e8d84ea2e9c5c70695518e0930775bf6b3200d14d99527`.
Manifest: `sha256:b17a140356f3c78204668aa385bd1d7bae2c946ac85cc0688e650f618a19fbfa`.

The built image ran its unchanged production worker command against synthetic
Redis, PostgreSQL tables and a minimal S3 HTTP fixture in a shared network-none
namespace. No host ports, real credentials, live data mounts or payments were
used. The fixture generated one second of blue video and audio.

- Idle worker received SIGTERM and exited 0.
- Active job download was held open. SIGTERM followed by SIGINT did not terminate
  it prematurely. After release it uploaded a 220-byte HLS manifest and a
  19,744-byte encrypted segment, stored exactly one content key and marked the
  first content completed. Worker exited 0.
- The queued successor remained waiting and its content remained pending.
- Installed runtime: Alpine 3.23.4, FFmpeg 8.0.1, Node 20.20.2.
- All five owned fixture containers were confirmed stopped. The exact container
  IDs, image identities, namespaces and volume boundaries are retained.

Cold fixture database initialization exceeded the original 90-second harness
allowance. Stopping during initialization left a valid cluster without the
requested dummy database. The next failed setup receipt is also retained. The
missing synthetic database was explicitly created before worker tests; the
harness allowance became 300 seconds. This is fixture setup, not PostgreSQL
startup or recovery acceptance. No production deadline changed.

Evidence directory:
`~/.local/state/archipelago/release-qualification/worker-runtime-29627fc-20261008/`

It contains runtime-receipt.json, qualification-boundary.json, both setup failure
receipts, image-source.json, container logs, and exact hashed Python/JavaScript
harnesses. Redis/PostgreSQL were selected from the retained legacy-images.tar;
their config SHA256 and every uncompressed layer digest were verified before
loading into the separate private build store. The original worker build receipt
still records runtimeQualified=false at build time; this later receipt records
the additional evidence without rewriting history.

This qualifies shutdown with real BullMQ/Redis, PostgreSQL and FFmpeg on synthetic
media. It does not qualify real MinIO behavior, deployment, production migration,
full-length media, producer payments, timed rentals or discovery. Scoped Yaya
image import and reviewed signed catalog inclusion remain gated by the IndeeHub
cutover/recovery acceptance work. No live image import or publication occurred.

## Prepared unsigned catalog amendment

A separate `unsigned-full-candidate-with-worker-29627fc.json` in the evidence
directory has SHA256 `9967d06c8809d69cce6057b9f45a630e9ce21fd5cd33541e352e23336cd8eb07`.
Its baseline remains unchanged at SHA256
`e217c73f76cd67fd7e9d96cd94fc4b4abd9f34ac35cce890dc59c02b30ada6a3`.
The exact semantic diff changes only the worker image to the qualified local
alias plus manifest digest and both worker version fields from 1.0.0 to 1.0.1.
The worker's API dependency has no version constraint; all dependency fields,
pull policy, root metadata and other app entries remain unchanged.
`worker-catalog-amendment-receipt.json` records the paths, before/after values,
hashes and source/runtime linkage. The preparation script is retained alongside.

The worker-only importer is prepared and root-reviewed as tooling. It refuses
execution unless explicitly invoked and the cutover/review gates are satisfied.
Its exact archive/image/digest/alias bindings and existing node preservation
checks remain required. Neither the catalog nor importer has been signed,
activated or executed against Yaya.
