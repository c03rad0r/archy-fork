#!/bin/sh
# Install the node-provided auth secret (64-char hex from the manifest's
# generated barkd-secret) so the wallet bridge can derive the matching Bearer
# token, then start the daemon. Without BARKD_SECRET, barkd generates its own
# random token in the datadir and the bridge won't authenticate — so treat a
# failed refresh as fatal rather than starting an unreachable daemon.
set -eu

if [ -n "${BARKD_SECRET:-}" ]; then
    # `secret refresh` prints the Bearer token on stdout — never log it.
    barkd secret refresh --secret "$BARKD_SECRET" >/dev/null
    unset BARKD_SECRET
fi

exec barkd
