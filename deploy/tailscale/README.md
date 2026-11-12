# Private Signet preview over Tailscale

Active URL: https://kaleido-edge.mole-pangolin.ts.net:8444/

Frontend source: a063e5dd77a6703bd77849c0c139c8362bb9b00c. Static build in `webapp/dist`, including `.well-known/rgb/tokens.json`.

## Topology and access

Tailscale Serve terminates HTTPS on the tailnet address, port 8444, and proxies to 127.0.0.1:3180. Nginx runs separately from the maker, non-root, read-only, without wallet mounts or credentials. Existing Serve routes on 443 and 8443 are preserved. Funnel is not enabled. Access follows the existing tailnet ACLs; this is no longer the Cloudflare single-email allowlist.

The browser still uses the existing public Nostr relay, RGB registry and Signet indexer. Only frontend access moved to VPN; no maker API was exposed. The extension requests normal connection approval for the new origin.

## Deploy

On the build machine, build `webapp` with Node 22 and copy the complete dist directory (including hidden files), compose.yml and nginx.conf to `/opt/kaleido/mosaik-preview` on kaleido-edge. Never copy .env files, wallet data or node_modules.

On the server:

```sh
sudo docker compose -f /opt/kaleido/mosaik-preview/compose.yml up -d
sudo tailscale serve --bg --https=8444 http://127.0.0.1:3180
```

Verify HTTPS 200, the JSON catalog, security headers and the loopback-only Docker port. Validate that public IPv4/IPv6 ports 8444 and 3180 are unreachable. Persisted Serve configuration and Docker restart policy survive restarts.

## Retired Pages preview

On 2026-11-12 the existing root/wildcard Access application was retained but its policy changed to deny everyone; old sessions were revoked. Do not remove that Access application while historical Pages deployments exist. No public beta was opened. No Pages resources were permanently deleted.

## Rollback

To stop the VPN route without affecting other services:

```sh
sudo tailscale serve --https=8444 off
sudo docker compose -f /opt/kaleido/mosaik-preview/compose.yml stop
```

Restore a previously verified frontend dist to roll back content. Any deliberate Pages fallback must first restore its saved owner-only Access policy, never remove protection.

## Validation 2026-11-12

QA extension 9f9d91af / audited WASM 51f318a sold 50 TESTUSD via this VPN URL. Order R3cWFtxfDHGKUwfUfgYm2uWE9szIl6Jm, transaction 41e740b4702cbe5d992a5a9491e6db81960cf49c432333542d5800fbc401076f, confirmed block 3583605. UI displayed Swap complete, TESTUSD 0 and 97,805 swap-eligible sats. No new funding or real-value assets used.
