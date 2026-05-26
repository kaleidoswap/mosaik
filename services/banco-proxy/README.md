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
pnpm install
pnpm run dev
```

then in another terminal:

```bash
curl -s -X POST http://localhost:3001/api/v1/auth/token \
     -H 'content-type: application/json' \
     -d '{"walletId":"maker"}' | jq .
```

> The proxy requires **pnpm**, not npm, because `@arkade-os/banco` is
> installed from git and runs a build hook at install time. Both
> behaviours are gated by `pnpm-workspace.yaml` in this directory.

## Docker

```bash
docker build -f services/banco-proxy/Dockerfile -t banco-proxy:dev services/banco-proxy
docker run --rm -p 3001:3001 banco-proxy:dev
```

In `docker/compose.arkade-mutinynet.yaml` it runs as `mosaik-banco-proxy`
on the `mosaik` network alongside arkd/introspector.

## Upstream caveat

`@arkade-os/banco` is alpha. The proxy works around this with a small,
contained set of install-time accommodations:

| | |
|---|---|
| **npm registry** | not published (404). The proxy installs from `github:arkade-os/banco` (default branch `master`). |
| **transitive dep** | pulls `@arkade-os/sdk` from `github:louisinger/wallet-sdk#arkade-script-final` — a personal fork on a branch ref. |
| **install hook** | runs `pnpm run build` to compile the SDK's source. Both git-deps must be in `pnpm-workspace.yaml`'s `allowBuilds`; the transitive fork-dep requires `blockExoticSubdeps: false`. Both flags live in `pnpm-workspace.yaml`. |
| **ESM imports** | upstream's `dist/esm/{index,maker,taker}.js` use extensionless relative imports (`from "./offer"`) which crash Node ESM. Our `postinstall` hook (`scripts/patch-banco-esm.mjs`) rewrites these to add `.js`. Five sites today; the patch is idempotent and self-removes the day upstream ships a working dist. |
| **typecheck + build** | ✅ pass against the real SDK surface. |
| **runtime** | ✅ daemon starts cleanly; `GET /healthz` and `POST /auth/token` serve. Live banco calls require `BANCO_WALLET_SEED_<ID>` env vars and a reachable arkd. |

When upstream cuts a release with a working ESM build, delete
`scripts/patch-banco-esm.mjs` and the `postinstall` hook in
`package.json` — nothing else changes.

## What this is not

- **Not an orderbook.** Discovery is `mosaik-relay-server` (Nostr kind
  30050) and, eventually, an arkd-tx-stream tap that surfaces on-chain
  offers.
- **Not a wallet UI.** That's the Vite SPA (`webapp/`).
- **Not the matching engine.** banco-proxy executes a single offer or
  fill; composing them into a trade is the `mosaik-api` layer's job.
