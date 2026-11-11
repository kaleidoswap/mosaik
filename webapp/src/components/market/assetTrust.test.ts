import { describe, expect, it } from 'vitest';
import { assetTrust } from './assetTrust';

const A = 'rgb:AAAAAAAA-1111';
const B = 'rgb:BBBBBBBB-2222';
const list = [{ contractId: A, ticker: 'TUSD', name: 'Test USD', listedBy: [] }];

describe('assetTrust', () => {
  it('is listed when the index knows the asset under the same ticker', () => {
    const t = assetTrust({ assetId: A, ticker: 'TUSD' }, [{ assetId: A, ticker: 'TUSD' }], list);
    expect(t.level).toBe('listed');
  });
  it('warns when the maker names an asset differently from the index', () => {
    const t = assetTrust({ assetId: A, ticker: 'USDT' }, [], list);
    expect(t.level).toBe('warning');
    expect(t.indexTicker).toBe('TUSD');
  });
  it('cautions when another asset on the page reads the same', () => {
    const t = assetTrust({ assetId: B, ticker: 'TU5D' }, [{ assetId: B, ticker: 'TU5D' }, { assetId: A, ticker: 'TUSD' }], list);
    expect(t.level).toBe('caution');
    expect(t.lookalikes).toEqual([A]);
  });
  it('warns on a ticker that mixes alphabets', () => {
    expect(assetTrust({ assetId: B, ticker: 'TUSТ' }, [], list).level).toBe('warning');
  });
  it('says unlisted for an unknown asset, without a verdict', () => {
    const t = assetTrust({ assetId: B, ticker: 'ZZZ' }, [], list);
    expect(t.level).toBe('unlisted');
  });
});

it('does not report an unavailable registry as unlisted', () => {
  const t = assetTrust({assetId: A, ticker: 'TUSD'}, [], undefined);
  expect(t.level).toBe('unknown');
  expect(t.notes.join(' ')).toContain('unavailable');
});
