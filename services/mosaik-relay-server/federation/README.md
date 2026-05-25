# mosaik-relay federation sidecar

`nostr-rs-relay` is a single-relay implementation; it doesn't reach out
to peers. This sidecar fills the gap with the smallest possible
contract:

> Subscribe to a set of peer relays, dedup, republish kinds
> 30050 / 30051 / 5 to the local Mosaik relay.

That makes a small federation of Mosaik relays look like one logical
orderbook to clients, without requiring any of the relays to fork
`nostr-rs-relay`.

## Mental model

```
┌─ peer relays ───────────────────┐         ┌─ this stack ────────────────┐
│ ws://other-mosaik-1:7777        │ ──REQ──▶│  mosaik-relay-federation    │
│ ws://other-mosaik-2:7777        │◀──EVT──│  (subscribes, dedupes,      │
│ wss://relay.example.com         │         │   republishes)              │
└─────────────────────────────────┘         │           │                  │
                                            │           ▼                  │
                                            │  mosaik-relay (nostr-rs)    │
                                            │  ws://localhost:7777        │
                                            └─────────────────────────────┘
```

## Env

| Var | Default | Meaning |
|---|---|---|
| `PEER_RELAYS` | (empty — sidecar idles) | Comma-separated peer relay ws:// URLs. |
| `LOCAL_RELAY` | `ws://mosaik-relay:7777` | The relay to republish to. Inside compose it's the service name. |
| `MAX_SEEN` | `50000` | Cap on the dedup set. FIFO eviction past this. |
| `LOG_LEVEL` | `info` | pino level. |

## Run

### Local dev

```bash
cd services/mosaik-relay-server/federation
npm install
PEER_RELAYS=wss://relay.damus.io,wss://nos.lol \
  LOCAL_RELAY=ws://127.0.0.1:7777 \
  npm run dev
```

### Docker

```bash
docker build -f services/mosaik-relay-server/federation/Dockerfile \
             -t mosaik-relay-federation:dev \
             services/mosaik-relay-server/federation

docker run --rm \
  -e PEER_RELAYS=wss://peer1.example,wss://peer2.example \
  -e LOCAL_RELAY=ws://mosaik-relay:7777 \
  --network mosaik \
  mosaik-relay-federation:dev
```

In compose: add as a sibling service with the same network as
`mosaik-relay`. Example overlay
(`docker/compose.federation.yaml`, not shipped — write per
deployment):

```yaml
services:
  mosaik-relay-federation:
    build:
      context: ../services/mosaik-relay-server/federation
    container_name: mosaik-relay-federation
    restart: unless-stopped
    depends_on:
      - mosaik-relay
    environment:
      LOCAL_RELAY: ws://mosaik-relay:7777
      PEER_RELAYS: ${PEER_RELAYS}
      LOG_LEVEL: info
    networks:
      - mosaik
```

## What it does not do (deliberate non-goals)

- **No outbound publishing of local events to peers.** This is
  pull-only. A separate sidecar (or just configuring clients to publish
  to multiple relays directly) handles that. Pull-only means peer
  takedowns / cancels / re-publishes propagate; arbitrary local writes
  do not leak.
- **No content validation.** That's the local relay's job (kind
  whitelist) and the publisher's job (schema check). The federator is
  transport, not policy.
- **No relay-discovery protocol.** Peer URLs are explicit (env). NIP-65
  / contact-list pulls are a fine future enhancement; today, operators
  curate.
- **No auth / NIP-42.** Add a `RELAY_TOKEN` and `Authorization:` header
  if you point at a private relay. Out of scope for v1.

## When you don't need this

- Single-relay deployments — trivially.
- Clients that already subscribe to multiple relays directly (most do).
  The federation sidecar exists to make the platform's *server-side*
  view coherent, not to replace client-side multi-relay subscription.
