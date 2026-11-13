# mosaik-webapp

The Mosaik covenant-DEX frontend — a Vite + React + TypeScript SPA that
drives `mosaik-api` over JSON-REST. Ships as `mosaik-webapp:dev` (nginx
on top of the Vite build), wired into `docker/compose.yaml` and exposed
on `:5173` in dev / prod.

## Stack

| | |
|---|---|
| Build | Vite 6 + TypeScript 5 |
| UI | React 18 |
| Data | TanStack Query 5 (polls `/api/state` every 2 s) |
| Styling | Tailwind 3, custom KaleidoSwap dark palette |
| Runtime image | `nginx:1.27-alpine` with envsubst-templated upstream |

## Run it

### Dev — Vite dev server with HMR

```bash
make webapp-dev                       # or:
cd webapp && npm install && npm run dev
```

Open <http://localhost:5173>. Vite proxies `/api` and `/healthz` to
`mosaik-api` (default `http://127.0.0.1:8080` — override with
`VITE_API_URL`).

### Prod — built SPA in nginx, full stack

```bash
make build-mosaik-webapp     # build the image
make dev-arkade              # full stack, webapp at http://localhost:5173
```

Inside compose the SPA's nginx proxies to `mosaik-api:8080` on the docker
network. Override with `MOSAIK_API_HOST` / `MOSAIK_API_PORT` env if you
deploy the image standalone (k8s, fly.io, …).

## What it shows today

- **Header**: active wallet switcher, block-height pill, backend
  selector chip (Liquid · Tessera / Arkade · banco). Switching backend
  is a UI-only state today; banco offers still travel through `mosaik
  make-offer --backend banco` until the API grows a `backend` param.
- **Order book**: every covenant offer with maker / pair / amounts /
  rate / UTXO. "Take" calls `/api/take-offer`; "Reclaim" calls the
  REFUND path on your own offers. Surfaces covenant rejections (cheats),
  successful settlements, and the rare covenant-bug case.
- **Make offer**: form for L-BTC/asset and asset/asset Liquid Tesseras,
  with a "Fund <wallet>" treasury button. Backend = banco shows a
  friendly nudge to use the CLI for now.
- **Trade tape**: settled trades this session.

## Listing tiers, maker badges (RGB market)

The RGB market reads two signed Nostr events from the relay and checks them in the browser
(`src/api/signed.ts`; formats in `docs/PROTOCOL.md`, kinds 30054 and 30055):

- **Listing tier** per market, by asset id only (`src/components/market/listing.ts`):
  *Kaleido curated* (in the kind-30055 list and its signature checks against the list key),
  *RGBMap listed* (in the registry), *Unlisted* (neither: shown with a warning and the asset id),
  or *Listing unknown* (registry unreachable). If the signed list cannot be checked, the bundled
  `/.well-known/rgb/tokens.json` is still used for names, but nothing is marked curated.
- **Verified maker** badge from the signed kind-30054 score, with the page's own thresholds.
- **Issuer** label when the signed list names the maker key under `issuer_makers` for that asset.
- **Makers on this market**: open orders, settled trades, distinct counterparties and volume per
  maker for this asset (30 days, verified on-chain, self-trades excluded).

| Variable | Default | |
|---|---|---|
| `VITE_KALEIDO_LIST_PUBKEY` | Mutinynet dev key `0edf1c09…1937` | Key the curated list must be signed by |
| `VITE_MOSAIK_INDEXER_PUBKEY` | same dev key | Key the maker scores must be signed by |
| `VITE_VERIFIED_MIN_SETTLED` | `3` | Badge: settled trades with a distinct counterparty |
| `VITE_VERIFIED_MIN_COUNTERPARTIES` | `2` | Badge: distinct counterparties |
| `VITE_VERIFIED_MIN_PRESENCE` | `0.9` | Badge: presence over 30 days |
| `VITE_VERIFIED_MAX_ABORT_RATE` | `0.2` | Badge: aborted negotiations |

**Open decisions:** the badge thresholds above are provisional (design doc, "Open decisions"),
and so is the production signing key for both events and where it is kept. The defaults are the
Mutinynet development key, not a production identity.

## Theme tokens (Tailwind)

- `ink-{400…900}` — the slate / void scale.
- `mosaic-cyan|violet|rose|amber|mint` — covenant accents.

Components live in `src/components/`; reach for `panel`, `chip-*`,
`btn-*`, `input`, `pill` utility classes in `src/styles.css` before
adding more bespoke styles.

## What's deliberately not here yet

- **Live Nostr subscription** to `mosaik-relay-server`. Today we poll
  `/api/state`; once the API exposes a WS stream that re-broadcasts
  relay events, swap it in.
- **Banco make/take through the webapp.** Routes need to grow a
  `backend` field on `/api/make-offer` and friends, plus a `BancoOffer`
  variant on the response. Tracked alongside Phase 2 follow-up.
- **Contract inspector panel** (covenant source, CMR, address). Was in
  the legacy HTML; will return as its own panel once we have an
  `/api/contract` parity in the new SPA.
- **Depth chart**, maker profiles (kind 30051), federated relay
  discovery — Phase 7 territory.

## File map

```
src/
├── main.tsx            React entry + TanStack Query provider
├── App.tsx             Layout shell + state hub
├── api/
│   ├── client.ts       fetch wrapper + the four CRUD hooks
│   └── types.ts        Hand-rolled mirror of mosaik-api's JSON shape
├── components/
│   ├── BackendBadge.tsx
│   ├── Header.tsx
│   ├── MakeOfferForm.tsx
│   ├── OrderBook.tsx
│   └── TradeTape.tsx
├── lib/
│   └── format.ts       Sats / rate / shortId helpers
└── styles.css          Tailwind + theme tokens
```
