# GitWorkshop upstream

This image packages the GitWorkshop NIP-34 web client from:

- Source: https://github.com/DanConwayDev/gitworkshop
- Pinned commit: `dc36db64f6a2cca29d109829eabaf0a49d4bf4da`
- Upstream project: https://gitworkshop.dev/
- App icon: `public/icons/icon.svg` from the same pinned revision (the artwork
  is only inset onto Archipelago's standard icon safe area).

The Archipelago integration patch only makes the upstream Vite/React
application work below Archipelago's `/app/archipelago-source/` mount, injects
the existing consent-gated Archipelago NIP-07 provider, disables the
development-only `localhost:4869` cache-relay probe, and removes two unreachable
lookup relays from the defaults. It does not replace GitWorkshop's NIP-34,
GRASP, repository browser, issue, pull-request, or review interfaces.

The separate dependency patch refreshes the npm lockfile and moves `fflate` to
0.8.3, `react-router-dom` to 7.18.3, and Vitest to 5.0.0. On 2026-09-30 the lockfile was refreshed again for `brace-expansion`
1.1.21/5.0.12, `fast-uri` 3.1.8 and `ip-address` 10.7.2 after fresh node
installs failed the retained dependency audit. The resulting clean install
reports zero npm advisories; its type-check, 152 unit tests, and Archipelago
subpath production build pass. The complete image also builds on the X250. Keeping this mechanical security
update separate makes both the upstream integration and future dependency
refreshes auditable.

The pinned revision and current upstream `main` do not contain a license file,
the package metadata declares no license, and GitHub reports no detected
license. Archipelago's owner explicitly accepted the resulting redistribution
risk on 2026-09-11. This is a project risk decision, not a claim that
GitWorkshop is licensed or that downstream recipients receive rights from its
copyright holders. An explicit upstream license remains the preferred,
auditable resolution.
