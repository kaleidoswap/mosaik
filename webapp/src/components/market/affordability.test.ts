import { describe, expect, it } from 'vitest';
import { shortfalls } from './affordability';

const sell = { side: 'sell' as const, amount: 100, priceSat: 3000, platformFeeSat: 1000, networkFeeSat: 2000 };

describe('shortfalls', () => {
  it('also funds the RGB receive output when buying', () => {
    expect(shortfalls({ ...sell, rgbOutputSat: 1000 }, { sat: 6000 })).toEqual([{ kind: 'sat', need: 7000, have: 6000 }]);
  });
  it('needs the price and the fees to buy from a maker', () => {
    expect(shortfalls(sell, { sat: 6000 })).toEqual([]);
    expect(shortfalls(sell, { sat: 5000 })).toEqual([{ kind: 'sat', need: 6000, have: 5000 }]);
  });
  it('needs the asset to sell into a buy order, and only the fees in sats', () => {
    const buy = { ...sell, side: 'buy' as const };
    expect(shortfalls(buy, { sat: 3000, asset: 100 })).toEqual([]);
    expect(shortfalls(buy, { sat: 3000, asset: 40 })).toEqual([{ kind: 'asset', need: 100, have: 40 }]);
  });
  it('asks for a free coin when the wallet has none', () => {
    expect(shortfalls(sell, { sat: 9999, freeColorableUtxos: 0 })).toEqual([{ kind: 'utxo' }]);
  });
  it('does not guess about what it does not know', () => {
    expect(shortfalls(sell, {})).toEqual([]);
  });
});
