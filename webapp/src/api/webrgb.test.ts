import { describe, expect, it } from 'vitest';
import { toBalance } from './webrgb';

describe('P2P balances', () => {
  it('uses wallet-verified reusable funding without summing all RGB funds', () => {
    expect(toBalance({ vanilla: { spendable: 3800, settled: 3800, future: 3800 }, colored: { settled: 133405 }, swapSpendable: 116205, freeColorableUtxos: 3 }))
      .toEqual({ spendable: 116205, total: 3800, rgbReserved: 133405, freeColorableUtxos: 3 });
  });
  it('keeps confirmed RGB change visible without treating it as normal swap funding', () => {
    expect(toBalance({ vanilla: { spendable: 58800, settled: 58800, future: 58800 }, colored: { settled: 132405 }, freeColorableUtxos: 3 }))
      .toEqual({ spendable: 58800, total: 58800, rgbReserved: 132405, freeColorableUtxos: 3 });
  });
  it('does not count pending change as confirmed swap funding or double count total', () => {
    expect(toBalance({ vanilla: { spendable: 40000, settled: 0, future: 40000 }, freeColorableUtxos: 1 }))
      .toEqual({ spendable: 0, total: 40000, freeColorableUtxos: 1 });
    expect(toBalance({ vanilla: { spendable: 30000, settled: 40000, future: 45000 }, freeColorableUtxos: 1 }))
      .toEqual({ spendable: 30000, total: 45000, freeColorableUtxos: 1 });
  });
});
