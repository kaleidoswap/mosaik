import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { generateSecretKey } from 'nostr-tools/pure';
import { bytesToHex, signerFromHex, type NostrEvent } from '../src/nostr.ts';
import { parseOffer } from '../src/offers.ts';
import { Store } from '../src/store.ts';
import { Indexer, SETTLE_DEADLINE_SECS, checkOrder } from '../src/indexer.ts';
import { loadConfig } from '../src/config.ts';
import type { Chain } from '../src/esplora.ts';
import type { EsploraTx } from '../src/settlement.ts';
import { verifyCatalogEvent, verifyReputationEvent } from '../src/events.ts';

const realOffer = JSON.parse(readFileSync(new URL('./fixtures/offer-event.json', import.meta.url), 'utf8')) as NostrEvent;
const swaps = JSON.parse(readFileSync(new URL('./fixtures/mutinynet-swaps.json', import.meta.url), 'utf8')) as EsploraTx[];
const sellTx = swaps.find((t) => t.txid.startsWith('96da17284c'))!;

test('a real kind-30053 event from the public relay parses, receive address included', () => {
  const o = parseOffer(realOffer);
  assert.ok(o);
  assert.equal(o.side, 'sell');
  assert.equal(o.network, 'signetcustom');
  assert.match(o.makerBtcAddress!, /^tb1p/);
  assert.equal(o.networkFeeSat, 2000);
  assert.equal(o.platformFeeSat > 0, true);
  assert.equal(o.feeScriptHex, '0014b43997d1acf0a9ede4418cbd5c0f55f8fb7a799c');
});

test('content that disagrees with the tags is refused', () => {
  const bad = { ...realOffer, tags: realOffer.tags.map((t) => (t[0] === 'price' ? ['price', '1'] : t)) };
  assert.equal(parseOffer(bad), null);
});

// a maker that sells for exactly what the fixture swap paid, so that swap settles the order
const maker = signerFromHex(bytesToHex(generateSecretKey()));
const indexerKey = signerFromHex(bytesToHex(generateSecretKey()));
// just before the fixture swap confirmed, so it falls inside the scoring window
const T0 = sellTx.status.block_time! - 600;

function offerEvent(id: string, status: 'active' | 'taken' | 'cancelled', at: number, extraTags: string[][] = []): NostrEvent {
  const offer = {
    swapId: id,
    makerGives: { kind: 'Rgb', assetId: 'rgb:9giqbeZi-Cs6oZhu-V9SoNP~-c90osSp-1Ebv0G5-Mv~4CY0', amount: 50 },
    makerReceives: { kind: 'Btc', assetId: null, amount: 27_148 },
    bitcoinNetwork: 'SignetCustom',
    networkFeeSat: 2000,
    rgbOutputSat: 1000,
    expirationTimestamp: T0 + 3600,
    makerBtcAddress: sellTx.vout[1]!.scriptpubkey_address,
    makerRgbScriptPubkeyHex: null,
    platformFeeSat: 1000,
    feeRecipientScriptPubkeyHex: '0014b43997d1acf0a9ede4418cbd5c0f55f8fb7a799c',
  };
  return maker.sign({
    kind: 30053,
    created_at: at,
    content: JSON.stringify({ offer, status, asset: { ticker: 'TESTXAU', name: 'Mosaik Demo Gold', precision: 4 } }),
    tags: [
      ['d', id],
      ['t', 'rgb-psbt-v1'],
      ['status', status],
      ['network', 'signetcustom'],
      ['side', 'sell'],
      ['asset', offer.makerGives.assetId],
      ['amount', '50'],
      ['price', '27148'],
      ['expiry', String(T0 + 3600)],
      ...extraTags,
    ],
  });
}

const chainWith = (txs: EsploraTx[]): Chain & { calls: string[] } => {
  const calls: string[] = [];
  return {
    calls,
    addressTxs: async (a) => (calls.push(`address:${a}`), txs.filter((t) => t.vout.some((o) => o.scriptpubkey_address === a))),
    scriptTxs: async (s) => (calls.push(`script:${s}`), txs.filter((t) => t.vout.some((o) => o.scriptpubkey === s))),
    tx: async (id) => (calls.push(`tx:${id}`), txs.find((t) => t.txid === id) ?? null),
  };
};

function setup(chain: Chain) {
  const store = new Store(':memory:');
  const cfg = { ...loadConfig({}), publish: false, catalogPath: new URL('../../../webapp/public/.well-known/rgb/tokens.json', import.meta.url).pathname };
  return { store, indexer: new Indexer(store, chain, indexerKey, cfg) };
}

test('active, then taken, then settled on-chain', async () => {
  const chain = chainWith([sellTx]);
  const { store, indexer } = setup(chain);
  assert.equal(indexer.ingest(offerEvent('order-1', 'active', T0), T0), 'offer');
  assert.equal(indexer.ingest(offerEvent('order-1', 'active', T0 + 60), T0 + 60), 'offer');
  assert.equal(indexer.ingest(offerEvent('order-1', 'taken', T0 + 90), T0 + 90), 'offer');
  // a late copy of an older version changes nothing
  assert.equal(indexer.ingest(offerEvent('order-1', 'active', T0 + 30), T0 + 100), 'stale');
  await indexer.runChecks(T0 + 100);
  const [o] = store.scoredOrders();
  assert.equal(o?.settleState, 'settled');
  assert.equal(o?.settlement?.txid, sellTx.txid);
  const [score] = indexer.scores(T0 + 1000);
  assert.equal(score?.negotiations.settled, 1);
  assert.equal(score?.volume.sat, 27_148);
  assert.equal(score?.volume.counterparties, 1);
});

test('taken, then cancelled, is an abort and is never looked up', async () => {
  const chain = chainWith([]);
  const { store, indexer } = setup(chain);
  indexer.ingest(offerEvent('order-2', 'active', T0), T0);
  indexer.ingest(offerEvent('order-2', 'taken', T0 + 30), T0 + 30);
  indexer.ingest(offerEvent('order-2', 'cancelled', T0 + 120), T0 + 120);
  await indexer.runChecks(T0 + 200);
  assert.equal(chain.calls.length, 0);
  const [o] = store.scoredOrders();
  assert.equal(o?.aborted, true);
  assert.equal(indexer.scores(T0 + 200)[0]?.negotiations.abortRate, 1);
});

test('cancelled without ever being taken is not a negotiation', () => {
  const { store, indexer } = setup(chainWith([]));
  indexer.ingest(offerEvent('order-3', 'active', T0), T0);
  indexer.ingest(offerEvent('order-3', 'cancelled', T0 + 60), T0 + 60);
  assert.equal(store.scoredOrders().length, 0);
});

test('taken with no settlement past the deadline is unsettled', async () => {
  const job = {
    maker: maker.pubkey,
    orderId: 'x',
    status: 'taken',
    expiry: T0 + 3600,
    takenAt: T0,
    lastSeen: T0,
    terms: parseOffer(offerEvent('x', 'taken', T0))!,
    txidHint: null,
    settleState: 'pending' as const,
    checks: 3,
  };
  assert.equal((await checkOrder(job, chainWith([]), T0 + 3600)).state, 'pending');
  assert.equal((await checkOrder(job, chainWith([]), T0 + SETTLE_DEADLINE_SECS + 1)).state, 'unsettled');
});

test('a txid tag is checked against the terms, not trusted', async () => {
  const chain = chainWith([sellTx]);
  const { store, indexer } = setup(chain);
  const wrong = swaps.find((t) => !t.txid.startsWith('96da'))!.txid;
  indexer.ingest(offerEvent('order-4', 'taken', T0, [['txid', wrong]]), T0);
  await indexer.runChecks(T0 + 10);
  assert.deepEqual(chain.calls, [`tx:${wrong}`]);
  assert.equal(store.scoredOrders()[0]?.settleState, 'pending');
});

test('an offer only ever seen active is still looked up once it has expired', async () => {
  const chain = chainWith([sellTx]);
  const { store, indexer } = setup(chain);
  indexer.ingest(offerEvent('order-5', 'active', T0), T0);
  await indexer.runChecks(T0 + 60);
  assert.equal(chain.calls.length, 0, 'not before expiry');
  await indexer.runChecks(T0 + 3600 + 601);
  assert.equal(store.scoredOrders()[0]?.settleState, 'settled');
});

test('forged signatures and other networks are ignored', () => {
  const { store, indexer } = setup(chainWith([]));
  const e = offerEvent('order-6', 'active', T0);
  assert.equal(indexer.ingest({ ...e, sig: e.sig.replace(/^./, e.sig[0] === 'a' ? 'b' : 'a') }, T0), 'ignored');
  const main = maker.sign({ ...e, tags: e.tags.map((t) => (t[0] === 'network' ? ['network', 'bitcoin'] : t)) });
  assert.equal(indexer.ingest(main, T0), 'ignored');
  assert.equal(store.makers().length, 0);
});

test('signing produces verifiable score and list events', async () => {
  const { store, indexer } = setup(chainWith([sellTx]));
  indexer.ingest(offerEvent('order-7', 'taken', T0), T0);
  await indexer.runChecks(T0 + 10);
  const events = await indexer.signAndPublish(T0 + 20);
  const rep = events.find((e) => e.kind === 30054)!;
  const list = events.find((e) => e.kind === 30055)!;
  assert.ok(verifyReputationEvent(rep, indexerKey.pubkey, T0 + 30).ok);
  assert.ok(verifyCatalogEvent(list, indexerKey.pubkey, 'signetcustom').ok);
  // the list is re-signed only when the catalog changes
  const again = await indexer.signAndPublish(T0 + 40);
  assert.equal(again.some((e) => e.kind === 30055), false);
  assert.equal(store.published(30055)[0]?.id, list.id);
});

test('presence counts fresh events at receive time; backdated ones fill nothing', () => {
  const { store, indexer } = setup(chainWith([]));
  const now = T0 + 10 * 86_400;
  indexer.ingest(offerEvent('order-8', 'active', now - 5 * 86_400), now); // written to look old
  assert.deepEqual(store.presenceOf(maker.pubkey, 0), []);
  indexer.ingest(offerEvent('order-8', 'active', now - 30), now);
  assert.deepEqual(store.presenceOf(maker.pubkey, 0), [Math.floor(now / 300)]);
});

test('taken, then active again, is an abort and keeps no escape from the deadline', async () => {
  const { store, indexer } = setup(chainWith([]));
  indexer.ingest(offerEvent('order-9', 'taken', T0), T0);
  indexer.ingest(offerEvent('order-9', 'active', T0 + 60), T0 + 60);
  const [o] = store.scoredOrders();
  assert.equal(o?.aborted, true);
  // and the settlement check itself goes by takenAt, whatever the latest status
  const job = { maker: maker.pubkey, orderId: 'y', status: 'active', expiry: T0 + 3600, takenAt: T0, lastSeen: T0 + 60,
    terms: parseOffer(offerEvent('y', 'active', T0))!, txidHint: null, settleState: 'pending' as const, checks: 5 };
  assert.equal((await checkOrder(job, chainWith([]), T0 + SETTLE_DEADLINE_SECS + 1)).state, 'unsettled');
});
