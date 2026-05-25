# Mosaik on Arkade / Mutinynet

A reproducible Mutinynet stack for the Arkade settlement backend. Brings up
bitcoind (signet w/ Mutinynet challenge) → nbxplorer → arkd-wallet → arkd →
introspector, plus `mosaik-api`. The banco-proxy and bancod services are
defined as commented stubs and land with their respective phases.

## Prerequisites

- Docker 24+ with the compose plugin.
- `make` (any version).
- ~5 GB free disk for the Mutinynet chain on first sync (smaller after pruning).
- A Mutinynet faucet visit to fund the maker / taker wallets:
  <https://mutinynet.com/faucet>.

## First-time setup

```bash
cd mosaik

# 1. Copy the env template and fill in the marked secrets.
cp docker/banco-mutinynet/.env.example docker/banco-mutinynet/.env
$EDITOR docker/banco-mutinynet/.env

# 2. Build the first-party images you need.
make build-mosaik-api

# 3. Bring the stack up.
make dev-arkade

# 4. Tail the slowest service while it syncs.
make logs SERVICE=bitcoind
```

The first run takes minutes — bitcoind has to fetch the (small) Mutinynet
chain, then nbxplorer indexes, then arkd-wallet imports descriptors. Once
`make ps` shows everything `healthy`/`running`, hit:

- `http://localhost:8080/healthz` → mosaik-api
- `http://localhost:7071/v1/info` → arkd admin
- `http://localhost:7073/health`  → introspector

## Funding the demo wallets

`scripts/mutinynet-bootstrap.sh` (TODO — Phase 2) will:
1. Create the `maker` and `taker` wallets in banco-proxy.
2. Print their faucet-target addresses.
3. Poll until the faucet sends land.

For now, derive an arkd-wallet address by hand:

```bash
# Get the wallet's base address
curl -s http://localhost:6060/v1/address | jq .
```

Paste that address into <https://mutinynet.com/faucet> and wait for one block.

## Stack layout

```
bitcoind (mutinynet signet)  :38332 RPC  :38333 P2P
        │
        ▼
nbxplorer                    :32838
        │
        ▼
arkd-wallet                  :6060
        │
        ▼
arkd                         :7070 gRPC  :7071 admin
        │
        ▼
introspector                 :7073
        │
        ▼ (Phase 2)
banco-proxy                  :3001
        │
        ▼
mosaik-api                   :8080
```

All services live on the `mosaik` docker network. Volumes:

- `bitcoind-data`  — chain state (largest)
- `nbxplorer-data` — wallet index
- `arkd-wallet-data` — keys, descriptors
- `arkd-data` — round state, VTXO tree

## Common commands

```bash
make dev-arkade       # bring up
make ps               # what's running
make logs SERVICE=arkd
make down             # stop, keep volumes
make nuke             # stop AND wipe volumes (destructive)
```

## Pinning + upgrades

All upstream image tags live in `docker/.versions.env`. To bump arkd:

```
ARKD_IMAGE=ghcr.io/arkade-os/arkd:<new-tag>
```

Then `make build-images && make dev-arkade`. The nightly e2e CI job
(coming in Phase 6) catches breakage.

## Open items

- `bancod` and `banco-proxy` services are commented out — they land in
  Phase 2 along with the `mosaik-banco` Rust crate.
- The Mutinynet bitcoind image (`ghcr.io/mutinynet/bitcoind`) is a
  community build; if the upstream tag moves, pin a specific digest.
- Introspector lives at `ghcr.io/arklabshq/introspector:v0.0.1` (a different
  org than arkade-os/*); track that release channel separately.
