import { describe, expect, it } from 'vitest';
import { mixesScripts, safeLabel, tickerSkeleton } from './text';

describe('safeLabel', () => {
  it('drops direction overrides, zero width and control characters', () => {
    expect(safeLabel('US‮DT​\u0007', 12)).toBe('USDT');
  });
  it('keeps to one trimmed line and the length', () => {
    expect(safeLabel('  a \n b  ', 12)).toBe('a b');
    expect(safeLabel('abcdefghijklmnop', 5)).toBe('abcde');
  });
  it('gives nothing for non strings and empty text', () => {
    expect(safeLabel(5, 4)).toBeUndefined();
    expect(safeLabel('​', 4)).toBeUndefined();
  });
});

describe('tickerSkeleton', () => {
  it('reads look-alikes as the same ticker', () => {
    expect(tickerSkeleton('USDT')).toBe(tickerSkeleton('U5DT'));
    expect(tickerSkeleton('USDT')).toBe(tickerSkeleton('USDТ')); // cyrillic T
    expect(tickerSkeleton('USDT')).toBe(tickerSkeleton('usdt'));
  });
  it('keeps different tickers apart', () => {
    expect(tickerSkeleton('USDT')).not.toBe(tickerSkeleton('TUSD'));
  });
});

describe('mixesScripts', () => {
  it('flags latin mixed with cyrillic or greek', () => {
    expect(mixesScripts('USDТ')).toBe(true);
    expect(mixesScripts('USDT')).toBe(false);
    expect(mixesScripts('ТЕСТ')).toBe(false);
  });
});
