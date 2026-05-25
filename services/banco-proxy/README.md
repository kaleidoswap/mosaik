# banco-proxy

Mosaik's HTTP gateway to [`@arkade-os/banco`](https://github.com/arkade-os/banco).
Arkade ships only TypeScript SDKs; this small Express service lets the Mosaik
Rust workspace (specifically `mosaik-banco`) drive banco swaps over REST.

## Status: SDK calls are stubbed

The HTTP contract is final and exercisable end-to-end. The SDK calls inside
`src/services/banco.ts` are stubs that return deterministic placeholders —
they let `mosaik-banco` integrate today against a stable shape, and let
`mosaik-api` route through banco offers in the orderbook UI, without
requiring a live arkd + introspector + alpha banco SDK install.

Why stub: `@arkade-os/banco` v0.0.1 pins a personal fork
(`github:louisinger/wallet-sdk#arkade-script-final`) and the surface is
explicitly alpha. The HTTP contract here is what we want long-term; the
SDK glue is what's volatile. Isolate the volatility, ship the contract.

**To swap stubs for real SDK calls** (after the Mutinynet stack is up and
banco hits a stable channel):

1. `npm install @arkade-os/banco @arkade-os/sdk` (or pin the fork explicitly).
2. Replace each `STUB:` block in `src/services/banco.ts`.
3. Add wallet construction in a `getWallet(walletId)` helper.
4. Run the e2e against the Mutinynet stack.

## REST surface

All routes are JWT-authed; mint a token first:

```
POST /api/v1/auth/token          { walletId }                 → { token, walletId }
```

Then for every banco call:

```
GET  /api/v1/banco/identity                                   → { walletPubkey, arkServerPubkey, introspectorPubkey }
POST /api/v1/banco/maker/offer   { wantAmount, wantAsset?,
                                    sellAmount, sellAsset?,
                                    cancelDelay, partialFill? }
                                                              → { offerHex, swapPkScript, fundingTxid, fundingVout }
POST /api/v1/banco/maker/cancel  { offerHex }                 → { txid }
GET  /api/v1/banco/maker/offers?swapPkScript=…                → [{ txid, vout, value, spendable }]
POST /api/v1/banco/taker/fulfill { offerHex, fillAmount? }    → { txid }
POST /api/v1/banco/taker/fulfill-by-txid { txid, fillAmount? } → { txid }
```

The Rust-side mirror is in `crates/mosaik-banco/src/client.rs` — keep both
in sync.

## Local dev

```bash
cd services/banco-proxy
npm install
npm run dev                       # tsx watch
# then in another terminal:
curl -s -X POST http://localhost:3001/api/v1/auth/token \
     -H 'content-type: application/json' \
     -d '{"walletId":"maker"}' | jq .
```

## Docker

```bash
docker build -f services/banco-proxy/Dockerfile -t banco-proxy:dev services/banco-proxy
docker run --rm -p 3001:3001 banco-proxy:dev
```

When uncommented in `docker/compose.arkade-mutinynet.yaml`, it runs as
`mosaik-banco-proxy` on the `mosaik` network alongside arkd/introspector.

## Operational notes

- `BANCO_PROXY_PORT` — default 3001.
- `BANCO_PROXY_JWT_SECRET` — JWT signing key. **Set in production.** The
  fallback string is intentionally obvious so it surfaces if forgotten.
- `LOG_LEVEL` — pino level (`trace|debug|info|warn|error`), default `info`.
- The known-wallets list is hard-coded for now (`maker`, `taker`, `mm-a`,
  `mm-b`). Replace with a mounted seed store before any non-dev deployment.

## What this is not

- **Not an orderbook.** Discovery is `mosaik-relay-server` (Nostr kind 30050)
  and, eventually, an arkd-tx-stream tap that surfaces on-chain offers.
- **Not a wallet UI.** That's the upcoming Vite SPA (`webapp/`).
- **Not the matching engine.** banco-proxy executes a single offer or fill;
  composing them into a trade is the `mosaik-api` layer's job.
