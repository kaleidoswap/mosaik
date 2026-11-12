/**
 * Recognising a p2pd swap on-chain from the public offer alone.
 *
 * rgb-lib builds every swap transaction the same way (`swap_build_psbt`, rgb-lib branch
 * ks/feat/onchain-swaps-stack, rev f4a35314):
 *
 *   inputs:  maker inputs, then taker inputs
 *   outputs: [0] OP_RETURN (value 0, the RGB commitment)
 *            maker BTC   (sell: `makerBtcAddress`, value = price)
 *            taker BTC   (buy: value = price)
 *            maker RGB   (buy: `makerRgbScriptPubkeyHex`, value = rgbOutputSat)
 *            taker RGB   (sell: value = rgbOutputSat)
 *            platform fee (when > 0: fee script, value = platformFeeSat)
 *            maker change (when > 0), then taker change (when > 0)
 *   fee:     exactly networkFeeSat (the taker funds it and the platform fee)
 *
 * The offer publishes a fresh receive script per order, so a confirmed transaction that pays it
 * with this exact layout is that order's settlement. The input sums then tell the maker's inputs
 * from the taker's, which is what counterparty and self-trade detection need.
 */

import type { ParsedOffer } from './offers.ts';

export interface EsploraTx {
  txid: string;
  fee: number;
  status: { confirmed: boolean; block_height?: number; block_time?: number };
  vin: { txid: string; vout: number; prevout: { scriptpubkey: string; value: number } | null }[];
  vout: { scriptpubkey: string; scriptpubkey_type?: string; scriptpubkey_address?: string; value: number }[];
}

export type SwapTerms = Pick<
  ParsedOffer,
  'side' | 'priceSat' | 'makerBtcAddress' | 'makerRgbScriptHex' | 'rgbOutputSat' | 'networkFeeSat' | 'platformFeeSat' | 'feeScriptHex'
>;

export interface SwapMatch {
  txid: string;
  confirmed: boolean;
  height: number | null;
  blockTime: number | null;
  /** Scripts of the inputs each side signed; null when the split is ambiguous. */
  makerInputScripts: string[] | null;
  takerInputScripts: string[] | null;
  makerChangeScript: string | null;
  /** The output the maker received on: its BTC address (sell) or its RGB witness script (buy). */
  makerReceiveScript: string;
}

const isOpReturn = (o: EsploraTx['vout'][number]) => o.value === 0 && o.scriptpubkey.startsWith('6a');

/** The settlement of these terms, if `tx` is one; null otherwise. */
export function matchSwapTx(terms: SwapTerms, tx: EsploraTx): SwapMatch | null {
  const out = tx.vout;
  if (tx.fee !== terms.networkFeeSat) return null;
  if (!out[0] || !isOpReturn(out[0])) return null;
  const pays = out[1];
  const rgb = out[2];
  if (!pays || !rgb || pays.value !== terms.priceSat || rgb.value !== terms.rgbOutputSat) return null;
  if (terms.side === 'sell') {
    if (!terms.makerBtcAddress || pays.scriptpubkey_address !== terms.makerBtcAddress) return null;
  } else {
    if (!terms.makerRgbScriptHex || rgb.scriptpubkey !== terms.makerRgbScriptHex) return null;
  }
  let next = 3;
  if (terms.platformFeeSat > 0) {
    const fee = out[3];
    if (!fee || fee.value !== terms.platformFeeSat || fee.scriptpubkey !== terms.feeScriptHex) return null;
    next = 4;
  }
  const change = out.slice(next);
  if (change.length > 2) return null;
  if (tx.vin.some((i) => !i.prevout)) return null;

  const values = tx.vin.map((i) => i.prevout!.value);
  const scripts = tx.vin.map((i) => i.prevout!.scriptpubkey);
  const makerNeeds = terms.side === 'sell' ? 0 : terms.priceSat + terms.rgbOutputSat;
  const takerNeeds =
    (terms.side === 'sell' ? terms.priceSat + terms.rgbOutputSat : 0) + terms.networkFeeSat + terms.platformFeeSat;

  // every split point that reproduces the change outputs; both sides bring at least one input
  const splits: number[] = [];
  for (let k = 1; k < values.length; k++) {
    const makerIn = values.slice(0, k).reduce((a, b) => a + b, 0);
    const takerIn = values.slice(k).reduce((a, b) => a + b, 0);
    const makerChange = makerIn - makerNeeds;
    const takerChange = takerIn - takerNeeds;
    if (makerChange < 0 || takerChange < 0) continue;
    const expected = [makerChange, takerChange].filter((v) => v > 0);
    if (expected.length === change.length && expected.every((v, i) => change[i]!.value === v)) splits.push(k);
  }
  if (splits.length === 0) return null;
  const k = splits.length === 1 ? splits[0]! : null;
  const makerHasChange = k !== null && values.slice(0, k).reduce((a, b) => a + b, 0) - makerNeeds > 0;

  return {
    txid: tx.txid,
    confirmed: tx.status.confirmed,
    height: tx.status.block_height ?? null,
    blockTime: tx.status.block_time ?? null,
    makerInputScripts: k === null ? null : scripts.slice(0, k),
    takerInputScripts: k === null ? null : scripts.slice(k),
    makerChangeScript: makerHasChange ? change[0]!.scriptpubkey : null,
    makerReceiveScript: terms.side === 'sell' ? pays.scriptpubkey : rgb.scriptpubkey,
  };
}

/** The first confirmed settlement among `txs`, else the first unconfirmed one, else null. */
export function findSettlement(terms: SwapTerms, txs: EsploraTx[]): SwapMatch | null {
  const matches = txs.map((t) => matchSwapTx(terms, t)).filter((m): m is SwapMatch => m !== null);
  return matches.find((m) => m.confirmed) ?? matches[0] ?? null;
}
