# Third-Party Licenses — neode-ui (web frontend)

Runtime dependencies bundled into the distributed web UI. Verified 2026-07-23.
Dev-only tooling (Vite, Playwright, TypeScript, etc.) is not distributed and
is not listed here. Note: the production bundler strips license header
comments, so this file (and the fonts' adjacent license files) constitute the
attribution shipped with the bundle; a build-time license-aggregation step is
planned (see docs/LICENSE-COMPLIANCE-AUDIT.md).

| Package | Version | License |
|---|---|---|
| vue | 3.5.x | MIT |
| vue-router | 4.6.x | MIT |
| vue-i18n | 11.3.x | MIT |
| pinia | 3.0.x | MIT |
| d3 | 7.9.x | ISC |
| leaflet | 1.9.x | BSD-2-Clause |
| @vue-leaflet/vue-leaflet | 0.10.x | MIT |
| dompurify | 3.4.x | MPL-2.0 OR Apache-2.0 (used under Apache-2.0) |
| buffer | 6.0.x | MIT |
| fast-json-patch | 3.1.x | MIT |
| fuse.js | 7.1.x | Apache-2.0 |
| qrcode | 1.5.x | MIT |
| qr-scanner | 1.4.x | MIT |
| qrloop | 1.4.x | MIT |

## Fonts

| Font | License | License file |
|---|---|---|
| Montserrat | SIL OFL 1.1 | public/assets/fonts/Montserrat/OFL.txt |
| Open Sans | Apache-2.0 | public/assets/fonts/Open_Sans/LICENSE.txt |

## Vendored

- `src/graphics/assets/grapheneos-boot-mask.png` — GrapheneOS boot artwork,
  Apache-2.0; see the adjacent ATTRIBUTION.md for the pinned source and credits.
- `public/assets/icon/` — see ATTRIBUTION.md in that directory
  (game-icons.net CC BY 3.0; pixelarticons MIT).
- `public/assets/img/mesh-devices/` — Meshtastic project artwork, GPL-3.0;
  see ATTRIBUTION.md in that directory.
