# App Icons – Canonical Source

**This is the single source of truth for all app icons.**

- **Path**: `neode-ui/public/assets/img/app-icons/`
- **Naming**: `{app-id}.{ext}` (e.g. `fedimint.png`, `mempool.webp`, `lnd.svg`)
- **Formats**: PNG, WebP, or SVG (prefer WebP for size, SVG for scalability)

All references in the codebase use `/assets/img/app-icons/{filename}`. Build outputs (web/dist, image-recipe) copy from here.

To add an icon: place the file here with the app-id as the filename. Run `npm run build` to update deployed assets.
