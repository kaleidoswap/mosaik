# banco-proxy

Mosaik's HTTP gateway to [`@arkade-os/banco`](https://github.com/arkade-os/banco).
Arkade ships only TypeScript SDKs; this small Express service lets the Mosaik
Rust workspace (specifically `mosaik-banco`) drive banco swaps over REST.

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

## Wallet seed configuration

Each `walletId` the proxy serves is backed by a mnemonic loaded from an
env var named `BANCO_WALLET_SEED_<ID>` (uppercased; non-alphanumeric chars
become `_`). For example:

```
BANCO_WALLET_SEED_MAKER="word1 word2 …"
BANCO_WALLET_SEED_TAKER="word1 word2 …"
```

For production, replace `getWallet` in `src/services/banco.ts` with a
call into the operator's KMS / mounted secret volume; the env-var loader
is a development convenience.

## Operational env

- `BANCO_PROXY_PORT` — default 3001.
- `BANCO_PROXY_JWT_SECRET` — JWT signing key. **Set in production.** The
  fallback in `src/auth.ts` is intentionally obvious so it surfaces if
  forgotten.
- `ARK_SERVER_URL` — arkd gRPC endpoint, default `http://arkd:7070`.
- `INTROSPECTOR_URL` — introspector endpoint, default `http://introspector:7073`.
- `LOG_LEVEL` — pino level, default `info`.

## Local dev

```bash
cd services/banco-proxy
npm install
npm run dev
```

then in another terminal:

```bash
curl -s -X POST http://localhost:3001/api/v1/auth/token \
     -H 'content-type: application/json' \
     -d '{"walletId":"maker"}' | jq .
```

## Docker

```bash
docker build -f services/banco-proxy/Dockerfile -t banco-proxy:dev services/banco-proxy
docker run --rm -p 3001:3001 banco-proxy:dev
```

In `docker/compose.arkade-mutinynet.yaml` it runs as `mosaik-banco-proxy`
on the `mosaik` network alongside arkd/introspector.

## Upstream caveat

`@arkade-os/banco` is alpha. Today the GitHub install runs upstream's
build hook and that build does not pass on a clean machine — there are
`implicit any` errors in `src/taker.ts` and the package still pins its
own `@arkade-os/sdk` dep to a personal-fork branch. Expect `npm install`
in this directory to fail until upstream publishes a tagged release
with a clean build.

The proxy's code is correct against the SDK's documented surface; flips
of the install commit / package version are the only thing standing
between today and a runnable banco-proxy. The CI job for this service
is `continue-on-error` until upstream stabilises (see
`.github/workflows/ci.yaml`).

## What this is not

- **Not an orderbook.** Discovery is `mosaik-relay-server` (Nostr kind
  30050) and, eventually, an arkd-tx-stream tap that surfaces on-chain
  offers.
- **Not a wallet UI.** That's the Vite SPA (`webapp/`).
- **Not the matching engine.** banco-proxy executes a single offer or
  fill; composing them into a trade is the `mosaik-api` layer's job.
