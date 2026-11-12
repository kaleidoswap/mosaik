import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BUCKET_SECS, DAY, buildClusters, presence, scoreMaker, type OrderRecord } from '../src/scoring.ts';
import type { SwapMatch } from '../src/settlement.ts';

const NOW = 1_800_000_000;
const A = 'a'.repeat(64);
const B = 'b'.repeat(64);
const s = (n: number) => '5120' + n.toString(16).padStart(64, '0');

let seq = 0;
function settled(maker: string, opts: { taker: string[]; makerIn?: string[]; receive?: string; change?: string | null; at?: number; price?: number }): OrderRecord {
  const at = opts.at ?? NOW - DAY;
  const settlement: SwapMatch = {
    txid: (++seq).toString(16).padStart(64, '0'),
    confirmed: true,
    height: 100 + seq,
    blockTime: at,
    makerInputScripts: opts.makerIn ?? [s(9000 + seq)],
    takerInputScripts: opts.taker,
    makerChangeScript: opts.change === undefined ? s(8000 + seq) : opts.change,
    makerReceiveScript: opts.receive ?? s(7000 + seq),
  };
  return {
    maker,
    orderId: `o${seq}`,
    side: 'sell',
    assetId: 'rgb:demo-asset-0001',
    amount: 100,
    priceSat: opts.price ?? 10_000,
    firstSeen: at - 600,
    lastSeen: at - 60,
    takenAt: at - 120,
    aborted: false,
    settleState: 'settled',
    settlement,
  };
}

function taken(maker: string, state: Partial<OrderRecord>): OrderRecord {
  seq++;
  return {
    maker,
    orderId: `t${seq}`,
    side: 'buy',
    assetId: 'rgb:demo-asset-0001',
    amount: 100,
    priceSat: 10_000,
    firstSeen: NOW - DAY,
    lastSeen: NOW - DAY,
    takenAt: NOW - DAY,
    aborted: false,
    settleState: 'pending',
    settlement: null,
    ...state,
  };
}

const allPresent = { present: [] as number[], observed: [] as number[] };

test('distinct counterparties: takers that share a coin are one counterparty', () => {
  const orders = [
    settled(A, { taker: [s(1), s(2)] }),
    settled(A, { taker: [s(2), s(3)] }), // shares s(2) with the first: same taker
    settled(A, { taker: [s(4)] }),
  ];
  const score = scoreMaker(A, orders, buildClusters(orders), allPresent, NOW);
  assert.equal(score.volume.trades, 3);
  assert.equal(score.volume.counterparties, 2);
  assert.equal(score.volume.sat, 30_000);
  assert.equal(score.volume.selfTrades, 0);
});

test('a taker paying with a coin the maker received is the maker itself', () => {
  const first = settled(A, { taker: [s(1)], receive: s(500) });
  // later the maker takes its own offer with the BTC it received on s(500)
  const wash = settled(A, { taker: [s(500), s(501)] });
  const orders = [first, wash];
  const score = scoreMaker(A, orders, buildClusters(orders), allPresent, NOW);
  assert.equal(score.volume.selfTrades, 1);
  assert.equal(score.volume.trades, 1);
  assert.equal(score.volume.sat, 10_000);
  assert.equal(score.settlements.find((t) => t.txid === wash.settlement!.txid)?.self, true);
});

test("a maker's change spent as a taker input is a self-trade too", () => {
  const first = settled(A, { taker: [s(1)], change: s(600) });
  const wash = settled(A, { taker: [s(600)] });
  const orders = [first, wash];
  assert.equal(scoreMaker(A, orders, buildClusters(orders), allPresent, NOW).volume.selfTrades, 1);
});

test('two maker keys that share coins count each other as self', () => {
  const a1 = settled(A, { taker: [s(1)], makerIn: [s(700)] });
  const b1 = settled(B, { taker: [s(2)], makerIn: [s(700)] }); // B signs a coin A also signed: one wallet
  const aTakesB = settled(B, { taker: [s(700)] });
  const orders = [a1, b1, aTakesB];
  const score = scoreMaker(B, orders, buildClusters(orders), allPresent, NOW);
  assert.equal(score.volume.selfTrades, 1);
  assert.equal(score.volume.counterparties, 1);
});

test("a taker's change pointed at the maker does not smear the maker", () => {
  // taker change scripts are never clustered: only signed inputs are
  const orders = [settled(A, { taker: [s(1)] }), settled(A, { taker: [s(2)] })];
  assert.equal(scoreMaker(A, orders, buildClusters(orders), allPresent, NOW).volume.selfTrades, 0);
});

test('unattributed trades are settled but add no volume', () => {
  const o = settled(A, { taker: [] });
  o.settlement!.takerInputScripts = null;
  o.settlement!.makerInputScripts = null;
  const score = scoreMaker(A, [o], buildClusters([o]), allPresent, NOW);
  assert.equal(score.negotiations.settled, 1);
  assert.equal(score.volume.unattributedTrades, 1);
  assert.equal(score.volume.trades, 0);
});

test('abort rate counts aborted and unsettled negotiations against resolved ones', () => {
  const orders = [
    settled(A, { taker: [s(1)] }),
    settled(A, { taker: [s(2)] }),
    taken(A, { aborted: true, settleState: 'none' }),
    taken(A, { settleState: 'unsettled' }),
    taken(A, { settleState: 'pending' }), // not resolved yet: not counted either way
  ];
  const n = scoreMaker(A, orders, buildClusters(orders), allPresent, NOW).negotiations;
  assert.deepEqual(n, { taken: 5, settled: 2, aborted: 1, unsettled: 1, pending: 1, abortRate: 0.5 });
});

test('trades outside the 30-day window do not count', () => {
  const old = settled(A, { taker: [s(1)], at: NOW - 31 * DAY });
  const score = scoreMaker(A, [old], buildClusters([old]), allPresent, NOW);
  assert.equal(score.negotiations.settled, 0);
  assert.equal(score.volume.sat, 0);
});

test('presence is the share of observed buckets with a heartbeat', () => {
  const b0 = Math.floor((NOW - DAY) / BUCKET_SECS);
  const observed = Array.from({ length: 100 }, (_, i) => b0 + i);
  const present = [...observed.slice(0, 95), b0 - 50 /* before coverage */, b0 + 500 /* indexer was down */];
  const p = presence({ present, observed }, NOW - 30 * DAY, NOW);
  assert.equal(p.observedBuckets, 100);
  assert.equal(p.presentBuckets, 95);
  assert.equal(p.ratio, 0.95);
  assert.equal(presence({ present, observed: [] }, NOW - 30 * DAY, NOW).ratio, null);
});

test('verified needs every threshold', () => {
  const b0 = Math.floor((NOW - DAY) / BUCKET_SECS);
  const observed = Array.from({ length: 10 }, (_, i) => b0 + i);
  const orders = [settled(A, { taker: [s(1)] }), settled(A, { taker: [s(2)] }), settled(A, { taker: [s(3)] })];
  const c = buildClusters(orders);
  assert.equal(scoreMaker(A, orders, c, { present: observed, observed }, NOW).verified, true);
  assert.equal(scoreMaker(A, orders, c, { present: observed.slice(0, 8), observed }, NOW).verified, false, 'presence 0.8');
  assert.equal(scoreMaker(A, orders.slice(0, 2), c, { present: observed, observed }, NOW).verified, false, 'two trades');
  const one = [settled(A, { taker: [s(1)] }), settled(A, { taker: [s(1)] }), settled(A, { taker: [s(1)] })];
  assert.equal(scoreMaker(A, one, buildClusters(one), { present: observed, observed }, NOW).verified, false, 'one counterparty');
});
