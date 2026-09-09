#!/bin/sh
# Install the exact ngit runtime validated for Archipelago source hosting.
#
# The GitHub release archive contains both `ngit` and `git-remote-nostr`.
# Keep version, filenames, and SHA-256 values together so image builds and
# OTA updates cannot silently resolve a newer upstream release.

set -eu

NGIT_VERSION="2.6.3"
NGIT_RELEASE_BASE="https://github.com/DanConwayDev/ngit-cli/releases/download/v${NGIT_VERSION}"
X86_64_ASSET="ngit-v${NGIT_VERSION}-x86_64-unknown-linux-gnu.2.17.tar.gz"
X86_64_SHA256="81dd9b6a11a4a0feb946e56f55d557dc24075f1dcdda00ac35f9fd01920b9779"
AARCH64_ASSET="ngit-v${NGIT_VERSION}-aarch64-unknown-linux-gnu.2.17.tar.gz"
AARCH64_SHA256="e9d9437b7574e729b5a5d5cd800ebd668b73e6eb5c5859d52f83114c2f4b08b8"

install_root="${ARCHIPELAGO_NGIT_INSTALL_ROOT:-}"
case "$install_root" in
    ""|/*) ;;
    *)
        echo "ARCHIPELAGO_NGIT_INSTALL_ROOT must be empty or absolute" >&2
        exit 2
        ;;
esac

install_dir="${install_root}/usr/bin"
ngit_bin="${install_dir}/ngit"
helper_bin="${install_dir}/git-remote-nostr"

if [ -x "$ngit_bin" ] && [ -x "$helper_bin" ] && \
   [ "$($ngit_bin --version 2>/dev/null || true)" = "ngit ${NGIT_VERSION}" ] && \
   [ "$($helper_bin --version 2>/dev/null || true)" = "v${NGIT_VERSION}" ]; then
    echo "ngit ${NGIT_VERSION} already installed"
    exit 0
fi

machine="${ARCHIPELAGO_NGIT_ARCH:-$(uname -m)}"
case "$machine" in
    x86_64|amd64)
        asset="$X86_64_ASSET"
        expected_sha256="$X86_64_SHA256"
        ;;
    aarch64|arm64)
        asset="$AARCH64_ASSET"
        expected_sha256="$AARCH64_SHA256"
        ;;
    *)
        echo "Unsupported ngit architecture: $machine" >&2
        exit 2
        ;;
esac

download_dir=$(mktemp -d -t archipelago-ngit.XXXXXX)
cleanup() {
    rm -rf -- "$download_dir"
}
trap cleanup EXIT HUP INT TERM

archive="$download_dir/$asset"
curl --fail --silent --show-error --location \
    --proto '=https' --tlsv1.2 \
    --retry 3 --connect-timeout 20 \
    --output "$archive" "$NGIT_RELEASE_BASE/$asset"

actual_sha256=$(sha256sum "$archive" | awk '{print $1}')
if [ "$actual_sha256" != "$expected_sha256" ]; then
    echo "ngit archive checksum mismatch for $asset" >&2
    echo "expected: $expected_sha256" >&2
    echo "actual:   $actual_sha256" >&2
    exit 1
fi

# Extract only the two expected top-level files. Unexpected archive content is
# never copied into the host filesystem.
tar -xzf "$archive" -C "$download_dir" ngit git-remote-nostr
mkdir -p "$install_dir"
install -m 0755 "$download_dir/ngit" "$ngit_bin"
install -m 0755 "$download_dir/git-remote-nostr" "$helper_bin"

[ "$($ngit_bin --version)" = "ngit ${NGIT_VERSION}" ]
[ "$($helper_bin --version)" = "v${NGIT_VERSION}" ]
echo "installed ngit ${NGIT_VERSION} for $machine"
