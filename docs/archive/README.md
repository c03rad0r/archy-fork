# docs/archive — historical records

Documents here are **finished history**: completed session logs, handovers,
point-in-time status snapshots, security audits of past versions, and design
docs whose feature has since shipped. They are kept for provenance and are
**not** maintained — nothing in this directory describes the current system.

For current state, start at:

- `docs/architecture.md` — as-built system architecture
- `docs/ROADMAP.md` — public-facing roadmap

| File | What it was | Why archived |
|------|-------------|--------------|
| `rust-orchestrator-migration.md` | Design for migrating container lifecycle from bash to Rust | Migration complete — `prod_orchestrator.rs` + `boot_reconciler.rs` are the live system |
| `demo-deployment-design.md` | Design for the public demo sandbox | Demo shipped; `docs/demo-build-info.md` is the live ops doc |
| `app-registry-status-2026-06-21.md` | Per-app migration snapshot from node .228 @ v1.7.99-alpha | Point-in-time snapshot; headline findings (immich legacy, meshtastic present) no longer true |
| `security-code-audit-2026-03.md` | March 2026 security audit of v0.1.0 (33 findings) | Historical record; top findings since remediated (Argon2id, persisted sessions, image verification) |
| `INSTALL-SCREENS-DESIGN.md` | Installer screen design solicitation | Installer implemented in `image-recipe/` |
| `three-mode-ui-design.md` | Design for the Pro/Easy/Chat three-mode UI | Fully implemented (`stores/uiMode.ts`, `EasyHome.vue`, `Chat.vue`, goals system) |
| `HANDOVER-2026-07-02-iso-feedback.md` | Session handover from the 2026-07-02 ISO feedback bug-bash | Completed session log |
| `SESSION-1.8.0-OTA-PROGRESS.md` | Working notes from the 1.8.0 OTA push | Completed session log |
