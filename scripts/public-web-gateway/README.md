# Owned public-web gateway admission

`policy.py` implements the frp server-plugin contract for an operator-owned
HTTPS passthrough gateway. The node-side app installs from the trusted catalogue; an operator provisions
the gateway separately and supplies the private enrollment file to Setup.

Run it bound to loopback alongside frps. Configure **all** operations `Login`,
`NewProxy`, `Ping`, `NewWorkConn`, and `NewUserConn`; omitting them weakens
revocation. Require TLS on frps and pin the gateway CA on every client. Retain
frp token authentication with `HeartBeats` and `NewWorkConns` additional scopes.
Do not publish its enrollment file, client config, or transport credentials.

The 0600 enrollment JSON maps a node name to `enabled`, the SHA-256 of a random
32-byte-or-longer `enrollment_token`, and exact lowercase `domains`. Set frpc
`user` to the node name and `metadatas.enrollment_token` to that token. Proxies
must be HTTPS with one assigned domain. TCP/UDP, wildcard subdomains, shared
proxy groups, and gateway-side content rewrites are refused. The node terminates
website TLS and owns its website keys. The gateway still observes SNI and traffic
metadata; passthrough is not an anonymity service.

Replace the policy file atomically to enroll, disable or rotate a node. Every
request reloads it; missing, malformed or nonprivate files fail closed. Disabling
an enrollment denies new connections and subsequent heartbeats. Already forwarded
bytes cannot be recalled; do not promise immediate termination of every stream.
The process never logs tokens or request bodies. Run behind a dedicated service
account with filesystem and process limits in the final deployment.

Tests: `python3 -m unittest discover -s tests/public-web-gateway -v`.

Contract references:
- https://gofrp.org/en/docs/features/common/server-plugin/
- https://gofrp.org/en/docs/features/common/network/network-tls/
- https://github.com/fatedier/frp/blob/v0.71.0/pkg/auth/token.go

The isolated Yaya qualification uses 17400 and14443, preserving existing public
sites. Its private certificate verifies TLS passthrough and ownership, not public
ACME issuance. Production ACME requires an appropriate public 443 route.

## Enroll a node

On the gateway, use the existing private frps JSON configuration and public CA
certificate. Keep the admission listener on loopback. For example:

```sh
python3 enroll.py --frps-config /etc/archy-gateway/frps.json \
  --policy /etc/archy-gateway/enrollments.json \
  --ca /etc/archy-gateway/gateway.crt \
  --host gateway.example.com --tls-server-name gateway.example.com \
  --name my-node --domain www.example.com \
  --output /secure/path/my-node-enrollment.json
```

Repeat `--domain` for separately assigned website/app names. Existing enrollments
require explicit `--rotate`; use a new output filename. Transfer the file privately
to the node owner. In Setup → Allow external connections → Public web, install
Public Web Router, choose the file, review the gateway/domains, and confirm.
Then connect a published website or a guest-enabled app to an assigned domain.
Neither importing the file nor connecting an app grants a guest token.

Set each public DNS A/AAAA record to the gateway's reachable public address.
The gateway needs its frps control port and a dedicated TCP 443 passthrough
listener. Public certificate issuance cannot be tested by pointing DNS at a
private LAN address or by using our isolated 14443 test port. If 443 already
serves other sites, retain that proxy and use a separately provisioned IP/path;
do not replace the existing listener blindly. Run frps and the policy as
persistent supervised services before production use. The Yaya qualification
services are intentionally isolated test services, not a production deployment.

For revocation, atomically replace the private policy with the node's `enabled`
set to false. For local disconnect, use Setup; it removes enrollment/routes while
preserving local certificates and drafts. Removing a route does not erase copies
of content that visitors previously downloaded.
