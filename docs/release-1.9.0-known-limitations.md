# 1.9.0-alpha: known limitations

## Angor historical discovery

Historical project discovery is incomplete. Funding commitments for all35
reference projects were verified, but34 original signed announcements were not
recovered from the relays and archives checked. They are not proven globally
lost. One recovered project passes Explore, details, statistics and public
TLS/WebSocket acceptance using the self-hosted services.

The operator explicitly accepted shipping with this limitation on2026-10-05.
Recovery remains follow-up work; this acceptance does not mean all-project
discovery passed. Retain the original relays alongside the self-hosted relay.

Full indexing requires a synced, unpruned Bitcoin node and indexing dependencies.
The dev node is still syncing; full-chain evidence comes from Shorty.

Evidence and inventory: [client acceptance](angor-client-acceptance-20261001.md),
[project recovery inventory](angor-project-recovery-20261001.json), and
[release acceptance](release-1.9.0-acceptance.md).

## Chat/contact storage migration — discovered during follow-up testing

A subsequent full backend test run exposed an existing random-prefix collision
in `storage_crypto::is_plaintext_json`: an encrypted store whose nonce begins
with `{` or `[` can be mistaken for legacy plaintext. This can prevent chat or
contact state loading correctly, and the message migration path can overwrite
that state. This helper is used for chat messages and mesh contact customizations,
not wallet databases.

The follow-up branch replaces the prefix-only heuristic with complete JSON
validation and adds deterministic encrypted-prefix regression cases. Qualification
is in progress; the published 1.9.0-alpha artifacts do not include that fix.
Existing release signatures and tags must not be silently replaced. Track a
corrective update and preserve affected state before further node restarts.
The earlier green release run did not detect this probabilistic failure; do not
interpret it as proof that this newly discovered issue is absent.
