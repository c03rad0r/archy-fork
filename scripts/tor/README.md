# Archipelago Tor Integration

Each service gets its own .onion address. Tor runs in a container with host networking so it can reach host-mapped ports.

## Service → Onion mapping

| Service    | LAN Port | Tor Hidden Service Dir        |
|-----------|----------|-------------------------------|
| Archipelago | 80      | hidden_service_archipelago   |
| LND UI    | 18083    | hidden_service_lnd           |
| BTCPay    | 23000    | hidden_service_btcpay        |
| Mempool   | 4080     | hidden_service_mempool       |
| Fedimint  | 8175     | hidden_service_fedimint      |

## Hostname files

After Tor starts, each service's .onion address is written to:
`/var/lib/archipelago/tor/hidden_service_<name>/hostname`

The backend reads these to expose Tor addresses in the package API.
