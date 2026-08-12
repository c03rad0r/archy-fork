#!/usr/bin/env bash
# build-bitcoin-image.sh — reproducible, verified, rootless Bitcoin image builder
# (docs/bitcoin-multi-version-design.md §3 Phase 0).
#
# Downloads an OFFICIAL upstream release tarball + SHA256SUMS(.asc), verifies the
# SHA-256 AND the OpenPGP signature (fail-closed), then builds a minimal rootless
# image and tags/pushes it to our registry as :<version>. Nodes only ever pull
# from our registry — they never fetch bitcoincore.org / bitcoinknots.org. The
# DHT Phase-0 catalog signature then carries provenance to the fleet.
#
# Usage:
#   scripts/build-bitcoin-image.sh core  31.0
#   scripts/build-bitcoin-image.sh knots 29.3.knots20260508
#   NO_PUSH=1 scripts/build-bitcoin-image.sh core 31.0   # build + verify only
#
# Env:
#   NO_PUSH=1          build + verify, do not push
#   ALLOW_UNSIGNED=1   skip the GPG signature check (NOT for production)
#   REQUIRE_PINNED=1   additionally require a signature from a pinned release key
#   ARCHY_REGISTRY     overrides the push registry (default from image-versions.sh)
set -euo pipefail

IMPL="${1:?usage: build-bitcoin-image.sh <core|knots> <version>}"
VERSION="${2:?usage: build-bitcoin-image.sh <core|knots> <version>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# shellcheck disable=SC1091
source "$ROOT/scripts/image-versions.sh"
REGISTRY="${ARCHY_REGISTRY:?ARCHY_REGISTRY unset}"

# Pinned upstream release-signing fingerprints (REQUIRE_PINNED=1 enforces these).
# Bitcoin Core SHA256SUMS for 25.x–31.x are signed by these maintainers; Knots by
# Luke Dashjr. Verified against the live signatures at build time.
# SHA256SUMS is a MULTI-signature file (every Guix builder signs it). We require
# a valid signature from at least one of these well-known release maintainers —
# the ones who sign every Bitcoin Core / Knots SHA256SUMS — and ignore builder
# sigs whose keys we don't hold. Both the primary fpr and the signing-subkey fpr
# that may appear in VALIDSIG are listed.
CORE_SIGNERS=(
  "0CCBAAFD76A2ECE2CCD3141DE2FFD5B1D88CA97D"  # fanquake (primary)
  "E777299FC265DD04793070EB944D35F9AC3DB76A"  # fanquake (subkey)
  "152812300785C96444D3334D17565732E08E5E41"  # achow101
  "71A3B16735405025D447E8F274810B012346C9A6"  # laanwj (older releases)
)
KNOTS_SIGNERS=(
  "1A3E761F19D2CC7785C5502EA291A2C45D0C504A"  # Luke Dashjr
)

case "$IMPL" in
  core)
    TARBALL="bitcoin-${VERSION}-x86_64-linux-gnu.tar.gz"
    BASEURL="https://bitcoincore.org/bin/bitcoin-core-${VERSION}"
    IMAGE_REPO="bitcoin"
    SIGNERS=("${CORE_SIGNERS[@]}")
    ;;
  knots)
    MAJOR="${VERSION%%.*}"
    TARBALL="bitcoin-${VERSION}-x86_64-linux-gnu.tar.gz"
    BASEURL="https://bitcoinknots.org/files/${MAJOR}.x/${VERSION}"
    IMAGE_REPO="bitcoin-knots"
    SIGNERS=("${KNOTS_SIGNERS[@]}")
    ;;
  *) echo "impl must be 'core' or 'knots'" >&2; exit 2 ;;
esac

TAG="${REGISTRY}/${IMAGE_REPO}:${VERSION}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
# podman/skopeo stage image copies under TMPDIR (default /var/tmp). Point it at a
# writable dir so `podman push` works in sandboxes where /var/tmp is read-only.
export TMPDIR="$WORK/tmp"; mkdir -p "$TMPDIR"

echo "==> [$IMPL $VERSION] downloading from $BASEURL"
curl -fsSL -o "$TARBALL"        "${BASEURL}/${TARBALL}"
curl -fsSL -o SHA256SUMS        "${BASEURL}/SHA256SUMS"
curl -fsSL -o SHA256SUMS.asc    "${BASEURL}/SHA256SUMS.asc"

echo "==> verifying SHA-256"
# SHA256SUMS lists every platform; check only our tarball line. Fail-closed.
grep " ${TARBALL}\$" SHA256SUMS | sha256sum -c - \
  || { echo "FATAL: SHA-256 mismatch for ${TARBALL}" >&2; exit 1; }

if [[ "${ALLOW_UNSIGNED:-0}" == "1" ]]; then
  echo "==> WARNING: ALLOW_UNSIGNED=1 — skipping GPG verification (NOT production)"
else
  echo "==> verifying OpenPGP signature on SHA256SUMS"
  # A persistent, pre-seeded keyring (BITCOIN_KEYRING_DIR) makes verification
  # reliable across many builds — keyserver fetches are flaky when each build
  # starts from an empty keyring. Falls back to a per-build keyring + fetch.
  if [[ -n "${BITCOIN_KEYRING_DIR:-}" ]]; then
    export GNUPGHOME="$BITCOIN_KEYRING_DIR"
  else
    export GNUPGHOME="$WORK/gnupg"
  fi
  mkdir -p "$GNUPGHOME"; chmod 700 "$GNUPGHOME"
  # Ensure each pinned maintainer key is present (best-effort fetch).
  for kid in "${SIGNERS[@]}"; do
    gpg --list-keys "$kid" >/dev/null 2>&1 && continue
    for ks in hkps://keys.openpgp.org hkps://keyserver.ubuntu.com hkp://keyserver.ubuntu.com; do
      gpg --keyserver "$ks" --recv-keys "$kid" >/dev/null 2>&1 && break || true
    done
  done
  # SHA256SUMS carries many builder signatures; `gpg --verify`'s exit code is
  # unreliable for multi-sig files (one unheld key flips it). Instead collect the
  # VALIDSIG fingerprints via --status-fd and REQUIRE at least one from a pinned
  # maintainer. Fail-closed otherwise.
  # `|| true`: gpg exits non-zero on multi-sig files even with good sigs; we
  # judge trust from VALIDSIG below, not the exit code (and set -e/pipefail would
  # otherwise abort here).
  VALID_FPRS="$(gpg --status-fd=1 --verify SHA256SUMS.asc SHA256SUMS 2>/dev/null \
                | awk '/^\[GNUPG:\] VALIDSIG/ {print $3; print $NF}' | sort -u || true)"
  ok=0; matched=""
  for fpr in $VALID_FPRS; do
    for want in "${SIGNERS[@]}"; do
      [[ "$fpr" == "$want" ]] && { ok=1; matched="$fpr"; }
    done
  done
  if [[ "$ok" != "1" ]]; then
    echo "FATAL: no valid signature from a pinned release maintainer on SHA256SUMS" >&2
    echo "       valid signers seen: ${VALID_FPRS:-none}" >&2
    exit 1
  fi
  echo "    verified: valid maintainer signature ($matched)"
fi

echo "==> extracting binaries"
tar -xzf "$TARBALL"
SRC="bitcoin-${VERSION}"
[[ -x "${SRC}/bin/bitcoind" ]] || { echo "FATAL: bitcoind missing in tarball" >&2; exit 1; }
mkdir -p ctx/bin
cp "${SRC}/bin/bitcoind" "${SRC}/bin/bitcoin-cli" ctx/bin/

echo "==> building rootless image $TAG"
cat > ctx/Containerfile <<'EOF'
FROM debian:bookworm-slim
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates; \
    rm -rf /var/lib/apt/lists/*; \
    useradd -m -u 1000 -s /bin/bash bitcoin; \
    mkdir -p /home/bitcoin/.bitcoin; \
    chown -R bitcoin:bitcoin /home/bitcoin
COPY bin/bitcoind /usr/local/bin/bitcoind
COPY bin/bitcoin-cli /usr/local/bin/bitcoin-cli
RUN chmod 0755 /usr/local/bin/bitcoind /usr/local/bin/bitcoin-cli
# Run as (container) root, exactly like the legacy hand-built :latest image.
# Rootless Podman maps container-root to the unprivileged host service user, and
# the manifest grants CAP_DAC_OVERRIDE so bitcoind can read its data dir — which
# the orchestrator chowns to the data_uid (host 100101 / container uid 102), NOT
# to this image's `bitcoin` user. A non-root USER here can't read existing chain
# data and bitcoind crash-loops with "Error initializing block database".
WORKDIR /home/bitcoin
VOLUME ["/home/bitcoin/.bitcoin"]
EXPOSE 8332 8333
ENTRYPOINT ["bitcoind"]
EOF
podman build -t "$TAG" ctx

echo "==> smoke test (bitcoind --version)"
podman run --rm --entrypoint bitcoind "$TAG" --version | head -1

if [[ "${NO_PUSH:-0}" == "1" ]]; then
  echo "==> NO_PUSH=1 — built + verified $TAG (not pushed)"
else
  echo "==> pushing $TAG"
  # The lfg2025 registry serves plain HTTP (matches image_uses_insecure_registry
  # in the Rust runtime). PODMAN_PUSH_TLS_VERIFY=true forces TLS for HTTPS regs.
  podman push --tls-verify="${PODMAN_PUSH_TLS_VERIFY:-false}" "$TAG"
  echo "==> pushed $TAG"
fi
