# Port Assignments Reference

This document lists all port assignments for Archipelago apps.

## Production Ports

| App | Port(s) | Protocol | Service | Dev Port(s) |
|-----|---------|----------|---------|-------------|
| bitcoin-core | 8332, 8333 | TCP | RPC, P2P | 18332, 18333 |
| btcpay-server | 80, 443 | TCP | HTTP, HTTPS | 10080, 10443 |
| home-assistant | 8123 | TCP | Web UI | 18123 |
| grafana | 3001 | TCP | Web UI | 13001 |
| endurain | 8085 | TCP | Web UI | 18085 |
| fedimint | 8173, 8174 | TCP | API, Web UI | 18173, 18174 |
| morphos-server | 8086 | TCP | Web UI | 18086 |
| lightning-stack | 9737, 10010, 8087 | TCP | P2P, gRPC, REST | 19737, 20010, 18087 |
| mempool | 4080 | TCP | Web UI | 14080 |
| ollama | 11434 | TCP | API | 21434 |
| searxng | 8888 | TCP | Web UI | 18888 |
| penpot | 8089 | TCP | Web UI | 18089 |
| lnd | 9735, 10009, 18080 | TCP | P2P, gRPC, REST | 19735, 20009, 28080 |
| core-lightning | 9736, 9835 | TCP | P2P, gRPC | 19736, 19835 |
| nostr-rs-relay | 8081 | TCP | HTTP/WebSocket | 18081 |
| strfry | 8082 | TCP | HTTP/WebSocket | 18082 |
| did-wallet | 8083 | TCP | Web UI | 18083 |
| router | 8084, 5353, 1900 | TCP/UDP | Web UI, mDNS, SSDP | 18084, 15353, 11900 |
| meshtastic | 4403, 1883 | TCP | HTTP API, MQTT | 14403, 11883 |

## Development Ports (Offset: +10000)

In development mode, all ports are offset by 10000 to avoid conflicts with production services.

### Quick Access URLs (Development)

| App | Dev URL |
|-----|----------|
| Bitcoin Core RPC | http://localhost:18332 |
| BTCPay Server | http://localhost:10080 |
| Home Assistant | http://localhost:18123 |
| Grafana | http://localhost:13001 |
| Endurain | http://localhost:18085 |
| Fedimint | http://localhost:18174 |
| MorphOS Server | http://localhost:18086 |
| Lightning Stack | http://localhost:18087 |
| Mempool | http://localhost:14080 |
| Ollama | http://localhost:21434 |
| SearXNG | http://localhost:18888 |
| Penpot | http://localhost:18089 |
| LND REST | http://localhost:18080 |
| Core Lightning | http://localhost:19835 |
| Nostr RS Relay | http://localhost:18081 |
| Strfry | http://localhost:18082 |
| DID Wallet | http://localhost:18083 |
| Router | http://localhost:18084 |
| Meshtastic | http://localhost:14403 |

## Port Conflict Resolution

All apps use unique base ports to prevent conflicts. The port offset system ensures:
- No conflicts in production (each app has unique ports)
- No conflicts in development (offset applied automatically)
- Easy port management via PortManager

## Changing Port Offset

The port offset is configurable via environment variable:

```bash
ARCHIPELAGO_PORT_OFFSET=10000
```

Or in the Archipelago config:

```toml
[dev]
port_offset = 10000
```

## Port Ranges

- **Bitcoin/Lightning**: 8000-10000 range
- **Web Services**: 3000-9000 range
- **System Services**: 10000+ range
- **Custom Apps**: 8000-9000 range
