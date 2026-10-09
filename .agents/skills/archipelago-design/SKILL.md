---
name: archipelago-design
description: Create or change Archipelago UI using the shared semantic tokens, components, responsive patterns, accessibility states, and embedded/standalone host contract.
metadata:
  short-description: Keep Archipelago UI consistent
---

# Archipelago design system

Use this skill for any new or changed Archipelago UI, including app screens,
Terminal, setup flows, dialogs, dashboards, and app wrappers. Read
[design-contract.md](references/design-contract.md) before implementation.

## Rules

- Find and reuse the closest shared component and pattern before creating one.
- Use semantic `--archy-*` tokens and the pinned kit version. A bespoke color,
  font, radius, shadow, or z-index needs a documented reason.
- Preserve the dark baseline, readable surfaces, 4px spacing rhythm, bottom
  action placement, 44px touch targets, visible focus, and safe-area behavior.
- Keep terminal/editor/tmux keys inside the terminal focus boundary; generic
  modal Escape/arrow handlers must not consume them.
- Implement loading, empty, offline, denied, validation-error, retry, disabled,
  and confirmed-success states. Status color must have text or icon support.
- Test standalone and embedded modes. Embedded apps do not duplicate the host
  wallpaper or navigation and must work without a host handshake.

## Workflow

1. Read the reference screen and source; choose the matching component/pattern.
2. Build with the shared kit and local assets, not runtime dashboard CSS or CDN
   fonts. Keep host RPC, signer, and credential bridges behind typed adapters.
3. Render the gallery/preview at required viewports and inspect screenshots and
   keyboard behavior. Check contrast on the real composited surface.
4. Record deliberate exceptions and update the kit only when a reusable need is
   demonstrated. Do not silently accept visual-diff changes.

## References

- [design contract](references/design-contract.md)
- [existing component map](references/component-map.md)
