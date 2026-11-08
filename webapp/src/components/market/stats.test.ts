import { describe, expect, it } from 'vitest';
import type { RgbFill } from '../../api/rgbp2p';
import { marketFills, marketStats, MAX_FILLS_PER_MAKER, plausibleFills } from './stats';

const fill = (at: number, priceSat: number, amount = 100, over: Partial<RgbFill> = {}): RgbFill => ({
  key: `m:${at}`, maker: 'm', side: 'sell', assetId: 'rgb:AAAAAAAA-1', amount, priceSat, network: 'signet', precision: 2, at, ...over,
});

describe('marketStats', () => {
  const now = 1_000_000;
  it('reads last price, change and volume over the last day', () => {
    const s = marketStats([fill(now - 100, 3300), fill(now - 5000, 3000), fill(now - 200_000, 1000)], now);
    expect(s.last).toBe(3300); // 3300 sat for 1.00 unit
    expect(s.change24h).toBeCloseTo(10);
    expect(s.volume24hSat).toBe(6300);
    expect(s.trades24h).toBe(2);
    expect(s.points.map((p) => p.price)).toEqual([1000, 3000, 3300]);
  });
  it('has no change with one fill, and nothing with none', () => {
    expect(marketStats([fill(now - 10, 3000)], now).change24h).toBeUndefined();
    const none = marketStats([], now);
    expect(none.last).toBeUndefined();
    expect(none.volume24hSat).toBe(0);
  });
  it('keeps to one asset on one network', () => {
    const f = [fill(1, 1), fill(2, 1, 100, { assetId: 'rgb:BBBBBBBB-2' }), fill(3, 1, 100, { network: 'testnet' })];
    expect(marketFills(f, 'rgb:AAAAAAAA-1', 'signet')).toHaveLength(1);
  });
});

describe('plausibleFills', () => {
  const now = 1_000_000;
  it('drops fills dated in the future', () => {
    expect(plausibleFills([fill(now + 10_000, 9)], now)).toEqual([]);
    expect(plausibleFills([fill(now + 60, 9)], now)).toHaveLength(1);
  });
  it('keeps only a maker\'s newest few', () => {
    const many = Array.from({ length: 30 }, (_, i) => fill(now - i, 100 + i));
    const kept = plausibleFills(many, now);
    expect(kept).toHaveLength(MAX_FILLS_PER_MAKER);
    expect(kept[0]!.at).toBe(now);
  });
});
