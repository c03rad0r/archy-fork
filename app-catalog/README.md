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

## Categories

money, commerce, data, home, nostr, networking, community, development, l484
