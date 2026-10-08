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

Validation checkpoint: 21 focused UI tests passed before the final background
resolver and pending-operation display additions. Those additions, the complete
app typecheck, responsive rendered-route/background checks and production build
remain queued behind IndeeHub recovery qualification. No live acceptance is claimed.
The backend peer-safety changes are isolated separately and also require tests and
paired helper deployment before these new mutation controls can be enabled.
