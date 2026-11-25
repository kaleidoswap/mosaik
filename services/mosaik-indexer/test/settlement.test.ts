import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { findSettlement, matchSwapTx, type EsploraTx, type SwapTerms } from '../src/settlement.ts';

// three confirmed swaps from Mutinynet, as Esplora returns them (trimmed to the fields used)
const txs = JSON.parse(readFileSync(new URL('./fixtures/mutinynet-swaps.json', import.meta.url), 'utf8')) as EsploraTx[];
const byPrefix = (p: string) => txs.find((t) => t.txid.startsWith(p))!;
const sellTx = byPrefix('96da17284c');
const buyTx = byPrefix('f5a0be7b50');
const FEE_SCRIPT = '0014b43997d1acf0a9ede4418cbd5c0f55f8fb7a799c';

const sellTerms: SwapTerms = {
  side: 'sell',
  priceSat: 27_148,
  makerBtcAddress: 'tb1p38366pkzykxnnhrhm0497zx3v28a7fq5s5dmzud6wqm4qydr32qssw49rz',
  makerRgbScriptHex: null,
  rgbOutputSat: 1000,
  networkFeeSat: 2000,
  platformFeeSat: 1000,
  feeScriptHex: FEE_SCRIPT,
};

const buyTerms: SwapTerms = {
  side: 'buy',
  priceSat: 49_000,
  makerBtcAddress: null,
  makerRgbScriptHex: buyTx.vout[2]!.scriptpubkey,
  rgbOutputSat: 1000,
  networkFeeSat: 2000,
  platformFeeSat: 1000,
  feeScriptHex: FEE_SCRIPT,
};

test('a sell is recognised and its inputs split between maker and taker', () => {
  const m = matchSwapTx(sellTerms, sellTx);
  assert.ok(m);
  assert.equal(m.txid, sellTx.txid);
  assert.equal(m.confirmed, true);
  // the maker brought the RGB coin (10 266 sat) and gets it back as change; the taker paid the rest
  assert.deepEqual(m.makerInputScripts, [sellTx.vin[0]!.prevout!.scriptpubkey]);
  assert.deepEqual(m.takerInputScripts, [sellTx.vin[1]!.prevout!.scriptpubkey]);
  assert.equal(m.makerChangeScript, sellTx.vout[4]!.scriptpubkey);
  assert.equal(m.makerReceiveScript, sellTx.vout[1]!.scriptpubkey);
});

test('a buy is recognised by the maker RGB script', () => {
  const m = matchSwapTx(buyTerms, buyTx);
  assert.ok(m);
  // maker: 51 000 in, 49 000 paid + 1 000 RGB output, 1 000 change; taker: 1 000 + 40 805 in
  assert.equal(m.makerInputScripts?.length, 1);
  assert.equal(m.takerInputScripts?.length, 2);
  assert.equal(m.makerChangeScript, buyTx.vout[4]!.scriptpubkey);
});

test('a transaction that differs from the terms in any way is not a settlement', () => {
  assert.equal(matchSwapTx({ ...sellTerms, priceSat: 27_149 }, sellTx), null, 'price');
  assert.equal(matchSwapTx({ ...sellTerms, makerBtcAddress: 'tb1pn6k50zu4ggv0gk08jt07nmaecfudp8jfsl2ajg3sw24337a65fqsm25qwk' }, sellTx), null, 'address');
  assert.equal(matchSwapTx({ ...sellTerms, platformFeeSat: 1386 }, sellTx), null, 'platform fee');
  assert.equal(matchSwapTx({ ...sellTerms, networkFeeSat: 2500 }, sellTx), null, 'network fee');
  assert.equal(matchSwapTx({ ...sellTerms, feeScriptHex: '0014' + '00'.repeat(20) }, sellTx), null, 'fee recipient');
  assert.equal(matchSwapTx({ ...buyTerms, side: 'sell', makerBtcAddress: sellTerms.makerBtcAddress }, buyTx), null, 'side');
  assert.equal(matchSwapTx(sellTerms, buyTx), null, 'another swap');
});

test('extra outputs break the layout', () => {
  const extra: EsploraTx = { ...sellTx, vout: [...sellTx.vout, { scriptpubkey: '51', value: 1 }] };
  assert.equal(matchSwapTx(sellTerms, extra), null);
});

test('a split that more than one input order explains leaves the trade unattributed', () => {
  // maker change 0 and taker change 0 at both k=1 and k=2 cannot happen with positive inputs, so
  // build one where two splits reproduce the same change outputs: inputs 0-sat padding are invalid
  // on-chain, but the matcher must still refuse to guess
  const tx: EsploraTx = {
    txid: 'aa'.repeat(32),
    fee: 2000,
    status: { confirmed: true, block_height: 1, block_time: 1 },
    vin: [
      { txid: 'bb'.repeat(32), vout: 0, prevout: { scriptpubkey: '5120' + '01'.repeat(32), value: 0 } },
      { txid: 'bb'.repeat(32), vout: 1, prevout: { scriptpubkey: '5120' + '02'.repeat(32), value: 0 } },
      { txid: 'bb'.repeat(32), vout: 2, prevout: { scriptpubkey: '5120' + '03'.repeat(32), value: 31_148 } },
    ],
    vout: [
      { scriptpubkey: '6a', value: 0 },
      { scriptpubkey: '5120' + '04'.repeat(32), scriptpubkey_address: sellTerms.makerBtcAddress!, value: 27_148 },
      { scriptpubkey: '5120' + '05'.repeat(32), value: 1000 },
      { scriptpubkey: FEE_SCRIPT, value: 1000 },
    ],
  };
  const m = matchSwapTx(sellTerms, tx);
  assert.ok(m);
  assert.equal(m.takerInputScripts, null);
  assert.equal(m.makerInputScripts, null);
});

test('findSettlement prefers a confirmed match', () => {
  const unconfirmed: EsploraTx = { ...sellTx, txid: 'cc'.repeat(32), status: { confirmed: false } };
  assert.equal(findSettlement(sellTerms, [unconfirmed, sellTx])?.txid, sellTx.txid);
  assert.equal(findSettlement(sellTerms, [unconfirmed])?.confirmed, false);
  assert.equal(findSettlement(sellTerms, [buyTx]), null);
});
