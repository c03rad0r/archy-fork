# DID Wallet

Web5 wallet with Decentralized Identifier (DID) support.

## Building

```bash
# From the apps directory
./build.sh did-wallet

# Or manually
cd did-wallet
docker build -t archipelago/did-wallet:latest .
```

## Development

```bash
cd did-wallet
npm install
npm run dev
```

## Ports

- **8083**: Web UI (dev: 18083)

## Running Locally

```bash
docker run -p 8083:8080 \
  -v /tmp/archipelago-dev/did-wallet:/app/wallet \
  -e DWN_ENDPOINT=http://localhost:13000 \
  archipelago/did-wallet:latest
```
