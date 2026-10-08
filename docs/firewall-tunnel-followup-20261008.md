# Firewall and tunnel follow-up — 8 October 2026

Status: source prepared in isolation; not deployed or accepted on a live node.
Task 18 remains open for full firewall rule management, persistence and rollback.

The Network entry uses a right-aligned status and the standard black glass-button.
The settings page fills the dashboard content width. The dashboard background
resolver now inherits the Network image for its detail routes instead of selecting
the Web5 image; the explicit federation background is preserved. Copy distinguishes
actual device tunnels, mesh connections, router settings and merely saved port
entries. A running mesh no longer produces a false Protected firewall label.

The new device section uses existing WireGuard APIs for add, reveal/copy and remove.
Mutation controls require the new backend's explicit peer_management_verified flag;
older or unavailable backends cannot enable them. Private configuration is fetched
only by an explicit reveal action, QR SVG is sanitized, and cached navigation clears
private details and rejects late replies. Failed creation/removal requires a fresh
list before retrying. Pending/revoking operations get recovery guidance.

Read-only operator-node checks confirmed an existing device tunnel and active mesh.
The separate router store had no connection or forwarding entries. This does not
contradict the separately retained manual firewall/mining repair. No live VPN,
firewall, router, app or payment change was performed during these checks.

Backend audit found that existing router add/remove-forward methods only write
local JSON. They are not exposed as controls that claim to open or close real ports.
The existing OpenWrt management screen remains linked. Actual node firewall rule
inspection/editing and the complete app exposure workflow are still separate work.

Validation: current focused tests pass 25/25 (22 component cases plus three
background resolver cases), and the full app typecheck passes. Typecheck exposed
an unsupported replaceAll call and a test-wrapper assertion; both were corrected
and the changed device test file was rerun successfully (16/16).

Source browser fixtures pass at 320, 390, 768 and 1440 pixels: full content width,
right-aligned values, no horizontal overflow, QR containment and unavailable
mutation controls against the old-backend fixture. A routed source fixture using
the same background resolver preserved the rendered Network background when
entering Firewalls & tunnels. This is not a full production Dashboard acceptance
or a live-node test. All RPC replies were synthetic; no node mutation occurred.

Local receipts:
- `/tmp/archy-firewall-focused-current-20261008.log` — three background tests.
- `/tmp/archy-firewall-focused-components-20261008.log` — 22 component tests.
- `/tmp/archy-firewall-device-final-20261008.log` — corrected device tests.
- `/tmp/archy-firewall-typecheck-20261008.log` — successful exit 0, no diagnostics.
- `/tmp/archy-firewall-responsive-20261008.log` — all four rendered viewport cases.
- `/tmp/archy-firewall-fixture-server.mjs` and
  `/tmp/archy-firewall-responsive.cjs` — the source fixture harnesses.

Production build and live acceptance remain pending. The backend peer-safety
changes are isolated separately; actual ephemeral-kernel helper qualification
passed, but Rust compilation/tests and paired helper deployment remain required
before the new mutation controls can be enabled on a node. Task 18 remains open
for broader host firewall management, persistence and rollback.
