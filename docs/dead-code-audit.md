# Dead-code audit

Status: first slice proposed, 2026-10-31.
This is not a “delete every warning” PR. The default build reports ~50
`dead_code` warnings; most are test-only helpers, serialized fields, or
features that are staged but not wired. Deleting them wholesale would remove
work in progress or change data/API shape.

## Why a first slice

A reviewable upstream change should remove only code that is provably unused
in **all** builds, or move test-only helpers under `#[cfg(test)]` so they stop
warning in the production build. Modules with `#![allow(dead_code)]`
(`streaming`, `wallet`, `swarm`) are explicitly WIP and are left alone.

## This slice

| Item | Location | Decision |
|---|---|---|
| `ELECTRUM` constant | `api/rpc/package/dependencies.rs` | Removed — no references |
| `save_new_file` + `save_new_file_with` + `save_after_direct_result` + `write_direct` + `write_via_userns` | `container/filebrowser.rs` | `#[cfg(test)]` — only unit tests call them; production uses `save_new_file_from` / `write_direct_stream` |
| `PortMap::is_empty` | `appgate/identity.rs` | `#[cfg(test)]` — only the manifest test calls it |
| `AssistantCounters::notices` | `assistant/mod.rs` | `#[cfg(test)]` — only the assistant loop tests call it |

## Kept deliberately

| Group | Examples | Why |
|---|---|---|
| Onchain/content features | `NativeSellerWallet`, `content_onchain_seller`, `save_catalog`, `publish_snapshot_onchain`, `allocate_onchain_offer` | Staged feature; removing deletes the implementation |
| LND purchase path | `LndPurchaseWallet::prepare_exact`, `transaction`, `max_signed_vsize`, `select_template`, `invoice_is_settled` | Optional/future purchase path |
| Container migration | `MigrationPlan::from_installed_render`, `from_reviewed_legacy`, `merge_reviewed_unit` | Migration/review path, used by tests today |
| Serde/API fields | `seller_onion`, `RoutstrProvider.models`, `PendingConfirmation.call_id`/`validated_args`, `Snapshot.key`, `PreparedRegistration.snapshot_path` | Removing changes data/API shape or loses retry/confirmation state |
| Config enums | `RestartPolicy::OnFailure` | May be parsed from config; not provably dead |
| Already-allowed modules | `streaming`, `wallet`, `swarm` | `#![allow(dead_code)]` with WIP comments |

## Verification

```bash
cargo check -p archipelago
cargo test -p archipelago container::filebrowser
cargo test -p archipelago appgate::identity
cargo test -p archipelago assistant
```

The warning count should drop for the touched modules; the remaining warnings
are intentional per the table above. Follow-up PRs can take one group at a
time, starting with any item that gains no runtime/test/serde consumer.
