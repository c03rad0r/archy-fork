# gashboard

> a custom dashboard for the datum gateway running on umbrel.
> for the four little boards that probably won't find a block, but are trying their best.

Polls a local Datum gateway (OCEAN's open-source mining gateway), shows live per-miner hashrate, share counts, lottery odds, and a tongue-in-cheek read on how unlikely it all is. Designed for a small fleet of solo-style hobby miners (Bitaxe, NerdQAxe, Avalon Nano 3, Avalon Mini 3) hashing into one Datum on Umbrel.

## What it shows

- **Pool hero** — combined hashrate, current block height being templated, mempool fee snapshot, datum connection status.
- **Per-miner cards** — nickname, ASIC type, location, live hashrate, accepted/rejected shares, last-share age, status light, firmware string from `useragent`.
- **Lottery widget** — P(block in 24h) at current network difficulty, with rotating self-deprecating commentary.
- **Live share ticker** — sparkline + scrolling feed of recent accepted shares.
- **Sats-today counter** — earnings projection from current hashrate × network reward.
- **Map panel** — pins for each location (split by remote IP from Datum's `/clients`), pulse-per-share.
- **Block celebration** — confetti + sound the day it finally happens.

## Auth

Nostr-only login (NIP-98 over HTTP). Two signers supported:
- **Remote signer** (NIP-46) — covers Primal app, Amber, nsecbunker.
- **Browser extension** (NIP-07) — Alby, nos2x, etc.

Allowlist of npubs is set via `NOSTR_ALLOWED_NPUBS`. Anything else is rejected before a session token is issued.

## Stack

- **Frontend** — Vue 3 + Vite + Pinia + TypeScript
- **Backend** — Express + TypeScript
- **Auth** — `nostr-tools` (NIP-98 verify), `applesauce-*` (signer abstraction, lifted from indeehub)
- **Container** — multi-stage `node:22.12.0-alpine` (pinned by SHA256 in Dockerfile)

## Deploy on Portainer (the way it'll actually run)

1. **Enable Datum admin API** on your Umbrel — edit
   `~/umbrel/app-data/datum-gateway/data/datum_gateway/datum_gateway_config.json`
   and add `"admin_password": "<openssl rand -hex 32>"` inside the `"api"` block.
   Restart the Datum app.

2. **Push this repo** to your gitea/whatever:
   ```bash
   git push -u origin main
   ```

3. **In Portainer → Stacks → Add stack → Repository**:
   - Repository URL: `https://git.tx1138.com/lfg2025/gashboard`
   - Compose path: `docker-compose.yml`
   - Add env vars (see `.env.example`)
   - Remove any old `DATUM_URL=http://10.21...`,
     `DATUM_URL=http://datum...`, or `DATUM_URL=http://datum_datum_1...` value
     from the stack env vars.
   - By default the stack uses host networking and polls Datum through Umbrel's
     host-published admin port: `DATUM_URL=http://127.0.0.1:21000`.
   - Reward contribution accounting is persisted in the Docker volume
     `gashboard_data` at `/data/contribution-ledger.json`.

4. Open the dashboard, log in with one of the allowed npubs, watch your boards lose at hashing in style.

## Local dev

```bash
pnpm install
pnpm dev    # runs api + web concurrently
```

## License

MIT.
