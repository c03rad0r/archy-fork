# GitWorkshop upstream

This image packages the GitWorkshop NIP-34 web client from:

- Source: https://github.com/DanConwayDev/gitworkshop
- Pinned commit: `dc36db64f6a2cca29d109829eabaf0a49d4bf4da`
- Upstream project: https://gitworkshop.dev/
- App icon: `public/icons/icon.svg` from the same pinned revision (the artwork
  is only inset onto Archipelago's standard icon safe area).

The small build patch only makes the upstream Vite/React application work
below Archipelago's `/app/archipelago-source/` mount, injects the existing
consent-gated Archipelago NIP-07 provider, disables the development-only
`localhost:4869` cache-relay probe, and removes two unreachable lookup relays
from the defaults. It does not replace GitWorkshop's NIP-34, GRASP, repository
browser, issue, pull-request, or review interfaces.

The pinned revision and current upstream `main` do not contain a license file,
the package metadata declares no license, and GitHub reports no detected
license. Archipelago's owner explicitly accepted the resulting redistribution
risk on 2026-09-11. This is a project risk decision, not a claim that
GitWorkshop is licensed or that downstream recipients receive rights from its
copyright holders. An explicit upstream license remains the preferred,
auditable resolution.
