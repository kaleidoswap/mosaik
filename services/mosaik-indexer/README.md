# mosaik-indexer

Measured maker reputation for the Mosaik RGB↔BTC order book, and the signed Kaleido curated asset
list. **Mutinynet (`signetcustom`) only**: the service refuses any other network.

It reads kind-30053 offers (`t=rgb-psbt-v1`) and kind-30051 maker profiles from
`wss://relay.kaleidoswap.com`, verifies fills on `https://esplora.signet.kaleidoswap.com`, and
signs:

- **kind 30054**, one maker reputation event per maker pubkey;
- **kind 30055**, the curated asset list built from `webapp/public/.well-known/rgb/tokens.json`.

Both formats are specified in [`docs/PROTOCOL.md`](../../docs/PROTOCOL.md) (kinds 30054 and 30055).
The reference encoder and verifiers are in [`src/events.ts`](src/events.ts); the verifiers need only
nostr-tools and can be copied into the webapp.

## Why TypeScript

The repo has both a Rust workspace and TypeScript services. The consumers of these events are
TypeScript (the webapp's `rgbp2p.ts` already parses kind 30053 with nostr-tools), so the
verifiers here can be shared with the webapp as they are. Node 22.18+ runs `.ts` directly and ships
SQLite, WebSocket, `fetch` and a test runner, so the only runtime dependency is nostr-tools and
there is no build step.

## Run it locally

Node ≥ 22.18 and pnpm.

```bash
cd services/mosaik-indexer
pnpm install
pnpm keygen        # Mutinynet dev key -> ~/.config/mosaik-indexer/mutinynet.key (mode 600)
pnpm test
pnpm once          # read the relay once, check settlements, print scores and the signed events
pnpm start         # keep running: subscribe, check, sign every 10 min, HTTP on 127.0.0.1:8787
```

Publishing to the relay is **off** unless `MOSAIK_INDEXER_PUBLISH=1`. With it off, events are signed,
stored and served over HTTP only.

| Variable | Default |
|---|---|
| `MOSAIK_INDEXER_RELAY` | `wss://relay.kaleidoswap.com` |
| `MOSAIK_INDEXER_ESPLORA` | `https://esplora.signet.kaleidoswap.com` |
| `MOSAIK_INDEXER_DB` | `services/mosaik-indexer/mosaik-indexer.db` (git-ignored) |
| `MOSAIK_INDEXER_KEY_FILE` | `~/.config/mosaik-indexer/mutinynet.key` |
| `MOSAIK_INDEXER_SECRET_HEX` | unset; overrides the key file (for a secret manager) |
| `MOSAIK_INDEXER_CATALOG` | `webapp/public/.well-known/rgb/tokens.json` |
| `MOSAIK_INDEXER_HTTP` | `127.0.0.1:8787` |
| `MOSAIK_INDEXER_PUBLISH` | `0` |
| `MOSAIK_INDEXER_SCORE_EVERY` | `600` seconds |
| `MOSAIK_VERIFIED_MIN_SETTLED`, `…_MIN_COUNTERPARTIES`, `…_MIN_PRESENCE`, `…_MAX_ABORT_RATE` | `3`, `2`, `0.9`, `0.2` |

### HTTP API (read only, CORS open)

| Route | Returns |
|---|---|
| `GET /health` | relay subscription state |
| `GET /v1/indexer` | signing pubkey / npub, kinds, thresholds, whether it publishes |
| `GET /v1/makers` | every maker's score (without the settlement list) |
| `GET /v1/makers/:pubkey` | one maker's full score and its kind-30051 profile, if any |
| `GET /v1/makers/:pubkey/event` | the signed kind-30054 event |
| `GET /v1/events/reputation` | all signed kind-30054 events |
| `GET /v1/assets` | the signed kind-30055 event and its catalog |

The JSON is computed on request; the signed events are as of the last scoring run. A client that
must not trust the API checks the signed events.

## Signing key

`pnpm keygen` writes a **development key for Mutinynet**, outside the repository, readable only by
the user. The service refuses a key file other users can read. Today the same key signs scores and
the curated list.

**Open decision:** which key signs scores and the curated list in production, whether they are two
keys, where they are kept (HSM, secret manager, an offline signer for the list) and how clients pin
and rotate them. Until that is settled, nothing should treat the dev key as more than a Mutinynet
test key.

## Scoring rules

All numbers cover the last 30 days and use public data only. The maker's own profile is shown, never
scored.

**Settled trade.** An order counts as settled only when a confirmed transaction on Esplora is its
swap. The public events do not carry the txid (it travels only in the private `broadcast` message),
so the indexer finds it from the offer itself: each offer publishes a fresh receive script for the
maker (`makerBtcAddress` on a sell, `makerRgbScriptPubkeyHex` on a buy), and rgb-lib builds every
swap with one layout (OP_RETURN; maker/taker BTC; maker/taker RGB output; platform fee; maker
change; taker change; maker inputs before taker inputs; fee exactly `networkFeeSat`). A transaction
paying that script with that exact layout is the settlement; anything else is not. See
[`src/settlement.ts`](src/settlement.ts). A `txid` tag on the offer, if p2pd adds one (proposal in
PROTOCOL.md), is checked against the same layout and never trusted on its own.

**One settlement, one maker.** Offers are public, so anyone can republish another maker's terms,
receive script included, under their own key and claim that maker's swap. A transaction is credited
only to the order the indexer *received* first (its own clock, not the event's `created_at`), and
only if that offer is dated before the transaction confirmed. Claims received at the same moment
(for example both backfilled at start-up) credit nobody. Losing claims are dropped from scoring:
neither trades nor aborts, and not clustered.

Orders are looked up when seen `taken` (then with backoff), and once after expiry when only `active`
was ever seen (in case the `taken` update was missed).

**Negotiations and aborts.** `taken` is published when the maker accepts a request, before any
signature. `taken` followed by `cancelled` (or by `active`, which p2pd never does) for the same
order is an abort (p2pd's `release`: taker
abort, proposal timeout, rejected completion, restart). `taken` with no settlement found within
48 hours is *unsettled*, counted with aborts. Abort rate = (aborted + unsettled) / (settled +
aborted + unsettled). Orders still pending are left out until they resolve.

**Presence.** Time is cut into 5-minute slots. A slot counts when the indexer was subscribed in it;
the maker is present in it if the indexer received a fresh kind-30053 event from it then (p2pd
republishes every open offer every 60 s). Presence is recorded at receive time, and only for events
dated within 5 minutes of it, so a backdated event cannot fill slots in which the maker was away. Presence = present slots / watched slots. A maker with nothing open is not present.

**Counterparties and self-trades.** From the layout the indexer knows which inputs the maker signed
and which the taker signed. It clusters coins by owner across all settled trades of all makers:
a maker owns its inputs, its change and its receive scripts; one taker's inputs in one transaction
share an owner. A trade whose taker cluster is the maker's own is a self-trade and adds no volume.
Two maker keys that ever sign the same coin become one owner, so trades between them are self-trades
too. Taker change and receive scripts are not used (a taker could point them at a maker to smear
it). If the input split is ambiguous the trade is *unattributed*: settled, but no volume.

**Volume** is the BTC leg (`price`) of settled trades with a distinct counterparty, plus per asset.

**Verified** (provisional thresholds, an open decision in the design doc): at least 3 such trades,
2 distinct counterparties, presence ≥ 0.9 and abort rate ≤ 0.2.

## What public data cannot show

- **Unanswered requests.** Requests are NIP-17 gift wraps; the relay shows only an ephemeral sender
  and the recipient. A request the maker never answered leaves no public trace, so it is not in the
  abort rate. A p2pd change would be needed (for example a public `requests` counter in the offer);
  none is proposed here.
- **Who aborted.** `taken` → `cancelled` does not say whether the taker or the maker walked away.
- **Wash trades with unlinked coins.** A maker that funds its taker wallet through transactions the
  indexer does not follow is not caught. Self-trade detection is a lower bound; distinct
  counterparties an upper bound.
- **History before the indexer ran.** The relay keeps only the latest event per order and deletes
  cancelled ones, so the indexer measures from when it starts. Presence is relative to the slots it
  watched.
- **Issuer-is-maker.** Not observable; the curated list carries it (`issuer_makers`) when we know.
