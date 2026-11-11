import { describe, expect, it } from 'vitest';
import { toBalance } from './webrgb';

describe('P2P balances', () => {
  it('does not count pending change as confirmed swap funding or double count total', () => {
    expect(toBalance({ vanilla: { spendable: 40000, settled: 0, future: 40000 }, freeColorableUtxos: 1 }))
      .toEqual({ spendable: 0, total: 40000, freeColorableUtxos: 1 });
    expect(toBalance({ vanilla: { spendable: 30000, settled: 40000, future: 45000 }, freeColorableUtxos: 1 }))
      .toEqual({ spendable: 30000, total: 45000, freeColorableUtxos: 1 });
  });
});
