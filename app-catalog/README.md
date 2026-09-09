# Archipelago App Catalog

Dynamic app catalog for the Archipelago marketplace. Nodes fetch this catalog to discover available apps.

## How it works

1. The Archipelago frontend fetches `catalog.json` from this repo
2. Apps listed here appear in every node's app store automatically
3. When a user installs an app, the backend pulls the Docker image and creates the container

## Adding a new app

Add an entry to `catalog.json`:

```json
{
  "id": "my-app",
  "title": "My App",
  "version": "1.0.0",
  "description": "What it does",
  "icon": "/assets/img/app-icons/my-app.svg",
  "author": "Author",
  "category": "data",
  "dockerImage": "source.archipelago-foundation.org/lfg2025/my-app:1.0.0",
  "repoUrl": "https://github.com/...",
  "containerConfig": {
    "ports": ["8080:8080"],
    "volumes": ["/var/lib/archipelago/my-app:/data"],
    "env": ["NODE_ENV=production"]
  }
}
```

For apps with hardcoded backend configs (Bitcoin, LND, etc.), `containerConfig` is optional.
For new apps, include `containerConfig` so the backend knows how to create the container.

## Storefront layout

Discovery merchandising is app-registry data, not node-OS layout. The optional
top-level `storefront` block defines the ordered Popular Apps rows and the
promotional banners placed before the remaining `All Apps` grid:

```json
{
  "storefront": {
    "popular": ["bitcoin-knots", "lnd", "btcpay-server"],
    "promotions": [{
      "id": "my-app",
      "banner": "/assets/img/featured/my-app.webp",
      "eyebrow": "open source",
      "headline": "Build together.",
      "description": "Catalog-controlled promotional copy.",
      "tag": "NOSTR // SOURCE",
      "launchLabel": "Open",
      "installLabel": "Install",
      "detailsLabel": "Learn more →"
    }]
  }
}
```

Only IDs present in `apps` render. New dashboards prefer `storefront` from the
daemon-verified signed catalog and use the bundled community copy as a local
fallback. `scripts/generate-app-catalog.sh` carries this block into the signed
release artifact; changing it does not require a node OS release once that
artifact is published.

## Categories

money, commerce, data, home, nostr, networking, community, development, l484
