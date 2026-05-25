# mosaik-relay-server

A curated [`nostr-rs-relay`](https://github.com/scsibug/nostr-rs-relay)
wrap, shipped as part of the Mosaik covenant-DEX platform.

## What's curated about it

`nostr-rs-relay` is a general-purpose relay. Mosaik only needs a tight
slice of it — the kinds the protocol defines:

| Kind | Event |
|---:|---|
| 30050 | Offer (addressable, replaceable) — the Tessera / banco offer JSON |
| 30051 | Maker profile (addressable, future) — pairs, fill rate, contact |
| 5     | Deletion (NIP-09) — used to cancel an offer |

The bundled `config.toml` whitelists exactly those kinds via
`event_kind_allowlist`. Anything else is rejected at admission. This:

- keeps the relay's storage tight (no random text notes piling up),
- makes the upcoming schema-validation hook tractable (only ever sees
  offer / profile / delete events),
- gives operators a predictable resource profile.

Retention defaults to 24 h for kind 30050 (offers usually expire much
sooner; the window is the safety net) and 7 days for kind 30051 (profiles
are longer-lived). Tune for your deployment.

## Building & running

Built as part of the base stack:

```bash
make build-mosaik-relay
make dev-arkade           # the relay comes up alongside mosaik-api
```

Standalone:

```bash
docker build -f services/mosaik-relay-server/Dockerfile \
             -t mosaik-relay-server:dev services/mosaik-relay-server

docker run --rm -p 7777:7777 mosaik-relay-server:dev
# WS endpoint: ws://localhost:7777
```

Sanity check from a client:

```bash
# Subscribe to all offers
websocat ws://localhost:7777 <<<'["REQ","sub-1",{"kinds":[30050]}]'
```

## What's not in this image (yet)

**Schema validation of offer content.** A relay-side hook that parses
`event.content` as JSON, runs it against the `OfferKind` JSON Schema
exported from `mosaik-core` (via `schemars`), and rejects malformed
offers at admission. Two design options under consideration:

1. A sidecar that subscribes to the relay, validates, and posts kind-5
   deletes for invalid events. Simple; no fork. Adds latency.
2. A small fork of nostr-rs-relay that adds a Rust validation hook
   reading the schema at startup. Strict; no latency. Maintenance burden.

For now, validation happens at the publisher (`mosaik-api`) before the
event is signed. The relay is content-agnostic but kind-restricted.

**Federation** is opt-in via the `federation/` sidecar in this
directory: a tiny Node process that subscribes to peer Mosaik relays
and republishes kind-30050/30051/5 events to this relay. nostr-rs-relay
itself stays vendor-pristine. See
[`federation/README.md`](federation/README.md) for the bring-up.

**Authentication.** The relay is open. To restrict makers to a known
set of pubkeys (e.g. paid market makers), populate
`[authorization].pubkey_whitelist` in `config.toml`.

## Operations

- Data lives in the `mosaik-relay-data` volume (sqlite file under
  `/usr/src/app/db`).
- Bump the upstream relay version in the Dockerfile's `RELAY_BASE` arg
  and `docker/.versions.env`.
- The relay's own logs go to stdout; `make logs SERVICE=mosaik-relay`.
