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
