# Bitcoin RPC Relay for External Wallets

This note captures the pattern used to let an external wallet, such as Wasabi,
use an Archipelago Bitcoin node for transaction relay without exposing the
node's admin RPC credentials.

## Goal

Expose a public HTTPS JSON-RPC endpoint that can broadcast transactions and read
basic chain/mempool state, while preventing wallet and admin RPC access.

The endpoint should be fronted by nginx or another TLS reverse proxy:

```text
wallet client -> https://<subdomain>/ -> reverse proxy -> Archipelago node nginx -> bitcoind RPC
```

Do not expose Bitcoin RPC credentials with wallet/admin access to external
users.

## Restricted RPC User

Create a separate RPC user, currently named `txrelay`, with an `rpcauth` secret
and a Bitcoin RPC whitelist.

Allowed RPC methods:

```text
sendrawtransaction
submitpackage
testmempoolaccept
getmempoolinfo
getrawmempool
getmempoolentry
getnetworkinfo
getblockchaininfo
getblockcount
getblockhash
getblockheader
getrawtransaction
gettxout
decoderawtransaction
decodescript
estimatesmartfee
```

Wallet/admin access is denied by setting `-rpcwhitelistdefault=0` and giving the
`txrelay` user only the method whitelist above.

Secrets live under:

```text
/var/lib/archipelago/secrets/bitcoin-rpc-txrelay-password
/var/lib/archipelago/secrets/bitcoin-rpc-txrelay-rpcauth
/var/lib/archipelago/secrets/bitcoin-rpc-txrelay-client.env
```

Do not commit these files or paste them into docs.

## Archipelago UI/API Flow

The productized flow is managed from the Bitcoin Core/Knots custom UI in the
`Transaction Relay Sharing` panel.

Implemented RPC methods:

```text
bitcoin.relay-status
bitcoin.relay-update-settings
bitcoin.relay-request-peer
bitcoin.relay-approve-request
bitcoin.relay-reject-request
bitcoin.relay-create-tor-service
```

When peer sharing is enabled, `bitcoin.relay-update-settings` automatically
provisions the restricted `txrelay` password, `rpcauth`, and client env file if
they do not already exist. If those files were just generated, restart Bitcoin
Core/Knots so `bitcoind` reloads the `txrelay` `rpcauth` and whitelist flags.

The UI shows:

```text
HTTP / HTTPS / Tor relay endpoint settings
local sync status
restricted credential readiness, without printing the password
trusted peer dropdown, disabled until the local node is synchronized
incoming relay requests with approve/reject actions
outbound relay requests and approval status
```

Approving an incoming peer request sends the selected endpoint plus restricted
`txrelay` credentials through the existing encrypted peer-message path. On the
requesting node, approved peer credentials are stored in a per-peer secret env
file:

```text
/var/lib/archipelago/secrets/bitcoin-relay-peer-<peer-pubkey-prefix>.env
```

The UI returns the credential secret path and approved endpoint metadata, but it
does not display the raw password.

For dev review, the mock server exposes the Bitcoin UI at:

```text
http://localhost:8102/app/bitcoin-ui/
```

## Bitcoin Startup Flags

The Bitcoin Knots app should add the restricted user only when the secret exists:

```sh
RPC_TXRELAY_AUTH="$(printenv BITCOIN_RPC_TXRELAY_RPCAUTH || true)"
RPC_TXRELAY_FLAGS="-rpcwhitelistdefault=0"
if [ -n "$RPC_TXRELAY_AUTH" ]; then
  RPC_TXRELAY_FLAGS="$RPC_TXRELAY_FLAGS -rpcauth=$RPC_TXRELAY_AUTH -rpcwhitelist=txrelay:sendrawtransaction,submitpackage,testmempoolaccept,getmempoolinfo,getrawmempool,getmempoolentry,getnetworkinfo,getblockchaininfo,getblockcount,getblockhash,getblock,getblockheader,getrawtransaction,gettxout,gettxspendingprevout,decoderawtransaction,decodescript,estimatesmartfee,uptime,ping,getconnectioncount,getpeerinfo,getindexinfo,getdeploymentinfo,getchaintips"
fi
```

Then include `$RPC_TXRELAY_FLAGS` in the `bitcoind` command. Keep the local
`archipelago` RPC user unrestricted for internal services by using
`-rpcwhitelistdefault=0` and only setting a whitelist for `txrelay`.

The current implementation touches:

```text
apps/bitcoin-knots/manifest.yml
scripts/container-specs.sh
```

## Node nginx

The Archipelago node can expose a host-based nginx vhost that proxies to local
Bitcoin RPC:

```nginx
limit_req_zone $binary_remote_addr zone=bitcoin_rpc_ext:10m rate=5r/s;

server {
    listen 80;
    server_name rpc.example.com;

    client_max_body_size 2m;

    location / {
        limit_req zone=bitcoin_rpc_ext burst=20 nodelay;
        limit_req_status 429;

        proxy_pass http://127.0.0.1:8332;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_connect_timeout 5s;
        proxy_send_timeout 120s;
        proxy_read_timeout 120s;
        proxy_buffering off;
    }
}
```

If another public reverse proxy terminates TLS, point it at:

```text
http://<archipelago-lan-ip>:80
```

For the tested node the LAN upstream was:

```text
http://archipelago.local:80
```

The public proxy should serve a valid TLS certificate for the chosen subdomain.

## DNS and Routing

Use a subdomain that resolves to the public reverse proxy:

```text
Type: A
Host/Name: <subdomain-only>
Value: <public-ip>
```

For example, if the desired hostname is `rpc.example.com`, the DNS host/name
field is usually only `rpc`, not the full `rpc.example.com`. Entering the full
hostname in some DNS panels can accidentally create:

```text
rpc.example.com.example.com
```

The public proxy should forward:

```text
TCP 443 -> TLS reverse proxy for the subdomain
TCP 80  -> optional, needed for HTTP-01 certificate issuance or redirects
```

If the public proxy is separate from the Archipelago node, configure it with:

```text
server_name: <subdomain>
scheme: http
upstream host: <archipelago-lan-ip>
upstream port: 80
```

## Verification

Check authoritative DNS:

```sh
dig @<authoritative-dns-ip> <subdomain> A +noall +answer +authority
dig @1.1.1.1 +short <subdomain> A
```

Check TLS:

```sh
openssl s_client -connect <subdomain>:443 -servername <subdomain> </dev/null
```

Check the public RPC path:

```sh
. /var/lib/archipelago/secrets/bitcoin-rpc-txrelay-client.env

curl -sS --user "$BITCOIN_RPC_TXRELAY_USER:$BITCOIN_RPC_TXRELAY_PASSWORD" \
  --data-binary '{"jsonrpc":"1.0","id":"check","method":"getblockchaininfo","params":[]}' \
  "<relay-endpoint-url>"
```

Check that transaction broadcast reaches Bitcoin RPC, without needing a real
transaction:

```sh
curl -sS --user "$BITCOIN_RPC_TXRELAY_USER:$BITCOIN_RPC_TXRELAY_PASSWORD" \
  --data-binary '{"jsonrpc":"1.0","id":"badtx","method":"sendrawtransaction","params":["00"]}' \
  "<relay-endpoint-url>"
```

Expected result is a Bitcoin RPC validation error such as `TX decode failed`,
which confirms the request reached `sendrawtransaction`.

If a wallet verifies the connection but reports `RPC Forbidden` during
broadcast, the credentials authenticated but the broadcast method was outside
the loaded `txrelay` whitelist. Restart the active Bitcoin backend after
updating the whitelist, then test both `sendrawtransaction` and, for newer
package-relay clients, `submitpackage`. Also confirm the public reverse proxy
passes the wallet's `Authorization` header through to `127.0.0.1:8332`; do not
point public wallet traffic at the Bitcoin UI `/bitcoin-rpc/` helper, because
that helper injects the local dashboard credential.

Check that wallet/admin RPC is blocked:

```sh
curl -sS -o /tmp/txrelay-deny.json -w '%{http_code}\n' \
  --user "$BITCOIN_RPC_TXRELAY_USER:$BITCOIN_RPC_TXRELAY_PASSWORD" \
  --data-binary '{"jsonrpc":"1.0","id":"deny","method":"listwallets","params":[]}' \
  "<relay-endpoint-url>"
```

Expected result:

```text
403
```

## Tested Outcome

The working endpoint used in this setup was:

```text
https://<your-mempool-instance>/
```

It was verified with:

```text
DNS resolves
TLS certificate is valid
txrelay credentials authenticate
getblockchaininfo returns chain=main
sendrawtransaction reaches Bitcoin RPC
listwallets is blocked for txrelay
```
