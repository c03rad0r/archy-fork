# Interactive hardware illustrations

The radio detection modal and unconfigured OpenWrt gateway use original
procedural Three.js models. The renderer follows the studio lighting,
rounded geometry, material finishes and instancing approach used for the
computer illustrations in `archipelago-website-deck`.

## Coverage

`neode-ui/src/graphics/deviceModels.ts` contains 25 radio illustrations,
including the generic fallback and three optional enclosure variants, plus
the Beryl AX (GL-MT3000). All existing `meshDeviceImages.ts` detection results
resolve to a model by their SVG filename. The existing SVGs and attribution
remain available as static fallbacks. USB chip identification remains a
best-effort visual match; it does not select firmware or claim exact hardware.

The radios include modeled rear faces, enclosure panels or populated boards,
and appropriate battery holders, connectors and antennas. These are stylized
recognition illustrations, not CAD models. Fine component placement and rear
details are representative; not every board revision or enclosure has been
verified against a rear photograph. The Beryl AX is the example router from
`docs/openwrt-gateway-setup.md`, not an assertion about the connected router.

Reference material:

- Existing Meshtastic illustrations in `neode-ui/public/assets/img/mesh-devices/`.
- Beryl AX official front/rear/side hardware drawing:
  https://static.gl-inet.com/docs/router/en/4/user_guide/gl-mt3000/hardware_info/mt3000_interface.jpg
- Beryl AX guide:
  https://docs.gl-inet.com/router/en/4/user_guide/gl-mt3000/
- T-Echo manufacturer board and enclosure reference:
  https://github.com/Xinyuan-LilyGO/T-Echo
  https://raw.githubusercontent.com/Xinyuan-LilyGO/T-Echo/main/image/T-ECHO.jpg
- T-Deck manufacturer documentation:
  https://wiki.lilygo.cc/products/t-deck-series/t-deck/

Manufacturer photographs are references only; none are copied into the app.
Geometry, grain, labels and screens are generated locally without CDN assets.

## Interaction and lifecycle

`DeviceModel.vue` loads the renderer on intersection. It turns once per minute,
capped at 30 rendered frames per second. Drag horizontally to rotate; a mouse
can also tilt vertically. Touch preserves vertical page scrolling. Arrow keys
rotate/tilt, Home shows the front and End shows the back. Keyboard interaction
pauses automatic turning. The Pause/Rotate button controls autoplay.

Reduced motion disables autoplay and hides its control while preserving manual
rotation. Hidden tabs and offscreen models stop scheduling frames. Model changes
reuse the renderer/environment and dispose the previous geometry, textures and
materials. Unmount releases the renderer and context. Unsupported browser APIs,
unavailable WebGL, import failures or lost contexts leave a static fallback.

The router figure uses the remaining viewport height below the connection form
on desktop (minimum 260px); phones get a fixed 330px illustration. Small desktop
windows can scroll normally rather than collapsing the model to zero height.

## Marketplace follow-up

The user requested a checkpoint after the models to switch to a cheaper model.
No Buy router button, relay searches, product cards or checkout implementation
are included in this change. Next: research the current NIP-99 specification,
add router marketplace discovery and product cards, then connect the purchase
entry point to Archipelago's existing checkout capabilities. The user explicitly
excluded building the whole checkout flow at this stage.

Browser scenarios and reports stay outside this repository, under
`/home/yaya/.local/share/browser-check/projects/archy-devices/`.

## Validation (2026-10-09)

- Production typecheck/build passes; the lazy renderer chunk is 136 KB gzipped.
- Existing gateway/cache tests (10) and radio setup tests (2) pass.
- Chromium 153, Firefox 155 and WebKit 26.6 pass at desktop 1440×900 and
  phone 390×844: all 26 models, front/back rotation, drag, autoplay/pause,
  reduced motion, zero offscreen/idle draws, model replacement, unmount/remount
  and context-loss fallback. Reports: `/tmp/archy-models-final/`.
- Actual gateway and detection-modal components pass the same six-case matrix
  with mocked RPC data. Reports: `/tmp/archy-device-pages-matrix/`.
- Delayed renderer loading, storage denial, reload and unavailable WebGL pass
  the same matrix. Reports: `/tmp/archy-device-fallbacks/`.
- These are Linux browser tests with phone viewport emulation, not physical
  iPhone/Android GPU or touch-scroll certification. No live router was changed.

Branch: `feat/3d-mesh-router`, based on ngit main
`5812f53c7c05027848a1007f818e9d33117b62e0`. The ngit helper could read relay
state but failed Git server authentication; main's published kind-30618 ref
was checked on relay.ngit.dev and that exact commit fetched from the existing
Git mirror. No remote proposal, merge or deployment was performed.
