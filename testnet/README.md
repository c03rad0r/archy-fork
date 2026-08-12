# Lightning Testnet Stack (Signet)

Real Bitcoin signet + LND + ThunderHub + Fedimint for testing Lightning features.

## Quick Start

```bash
docker compose -f docker-compose.testnet.yml up -d
```

First run takes ~10 minutes for signet blockchain sync (~200MB).

## Access

| Service | URL | Credentials |
|---------|-----|-------------|
| ThunderHub | http://localhost:3010 | password: `thunderhub` |
| Fedimint Guardian UI | http://localhost:18175 | — |
| LND REST API | http://localhost:8080 | — |
| Bitcoin RPC | localhost:38332 | user: `bitcoin` / pass: `bitcoinpass` |

## Get Signet Coins

1. Get a new address: `docker exec archy-lnd-signet lncli --network=signet newaddress p2wkh`
2. Visit https://signetfaucet.com and paste the address
3. Wait for 1 confirmation (~10 min)
4. Check balance: `docker exec archy-lnd-signet lncli --network=signet walletbalance`

## Open a Lightning Channel

```bash
# Connect to a signet peer (example: ACINQ signet node)
docker exec archy-lnd-signet lncli --network=signet connect 03...@signet-node:9735

# Open channel (amount in sats)
docker exec archy-lnd-signet lncli --network=signet openchannel --node_key=03... --local_amt=100000
```

Or use ThunderHub's UI at http://localhost:3010 to manage channels visually.

## Create & Pay Invoices

```bash
# Create invoice
docker exec archy-lnd-signet lncli --network=signet addinvoice --amt=1000 --memo="test payment"

# Pay invoice (from another node or ThunderHub)
docker exec archy-lnd-signet lncli --network=signet payinvoice <bolt11>
```

## Useful Commands

```bash
# Node info
docker exec archy-lnd-signet lncli --network=signet getinfo

# List channels
docker exec archy-lnd-signet lncli --network=signet listchannels

# Check Bitcoin sync progress
docker exec archy-bitcoind-signet bitcoin-cli -signet -rpcuser=bitcoin -rpcpassword=bitcoinpass -rpcport=38332 getblockchaininfo

# View logs
docker compose -f docker-compose.testnet.yml logs -f lnd-signet
docker compose -f docker-compose.testnet.yml logs -f thunderhub-signet

# Stop everything
docker compose -f docker-compose.testnet.yml down

# Reset all data (fresh start)
docker compose -f docker-compose.testnet.yml down -v
```

## Architecture

```
bitcoind-signet (port 38332)
    ↓ RPC + ZMQ
lnd-signet (gRPC 10009, REST 8080, P2P 9735)
    ↓ macaroon + TLS
thunderhub-signet (web UI on 3010)

bitcoind-signet
    ↓ RPC
fedimint-signet (Guardian UI on 18175, API on 18174)
```

All containers share the `signet-net` bridge network for internal DNS resolution.
