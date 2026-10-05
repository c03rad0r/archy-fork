# Private signed-catalog qualification

Use this only on explicitly selected development/acceptance nodes. It permits
testing a release-root-signed catalog before fleet publication. It does not
publish an app image, change the update mirrors, replace the trust anchor, or
authorize an unsigned catalog.

Set `ARCHY_APP_CATALOG_CANDIDATE` in a management-service systemd drop-in to an
absolute local catalog path. Keep that file readable by the service and outside
temporary storage if testing reboot persistence. The file must be at most 4 MiB
and carry a signature verified against the configured release-root anchor.
Malformed, missing, unsigned, tampered and wrong-key candidates fail before
replacing the previous cached bytes. An invalid explicitly selected candidate
does not fall back to the public catalog. The previous cache remains available;
inspect the refresh error rather than assuming the candidate was accepted.

Before activation, record the exact candidate hash, service binary hash, native
Bitcoin/LND identities and start times, app configuration, and existing catalog
cache/drop-ins. Back up persistent state before any app runtime migration.
Verify the candidate signature with `archipelago ceremony verify PATH` and
retain the original signed bytes. A private signing ceremony is not publication
approval.

After management restart, verify:

- Cached bytes exactly equal the signed candidate and still verify.
- Desired app manifests select the expected capability-compatible variant.
- Changed apps migrate through their supported lifecycle, with state backups.
- Native wallets, intentionally stopped/uninstalled apps and unrelated services
  retain their previous state.
- App requests succeed from the actual caller/container namespace; container
  health alone is insufficient.
- Repeated reconciliation, app restart, and separately arranged node reboot
  preserve routing, state, certificates and management isolation.

The setting intentionally pins catalog selection. Track its removal as part of
release completion: after the tested catalog is published and verified, remove
only the qualification drop-in, reload systemd, restart management, and confirm
the normal public refresh returns the expected signed catalog. Do not leave the
override behind to silently prevent future app updates. For an aborted test,
restore the reviewed previous catalog/runtime/configuration together; removing
the override alone can reintroduce older manifest settings.
