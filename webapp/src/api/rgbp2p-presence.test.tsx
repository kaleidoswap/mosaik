import { act, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const mock = vi.hoisted(() => ({ subs: [] as { filter: any; handlers: any }[] }));
vi.mock('nostr-tools', () => ({
  SimplePool: class {
    subscribeMany(_relays: unknown, filter: unknown, handlers: unknown) {
      mock.subs.push({ filter, handlers });
      return { close: () => {} };
    }
    close() {}
  },
}));

import { AWAY_AFTER, GONE_AFTER, lastSeen, liveness, useRgbP2pBook } from './rgbp2p';

const MAKER = 'a'.repeat(64);
const OTHER = 'b'.repeat(64);

function offerEvent(d: string, createdAt: number, pubkey = MAKER) {
  return {
    id: d, pubkey, kind: 30053, created_at: createdAt, sig: '',
    content: JSON.stringify({ offer: {}, asset: { ticker: 'TESTUSD', precision: 2 } }),
    tags: [['d', d], ['t', 'rgb-psbt-v1'], ['status', 'active'], ['network', 'signetcustom'], ['side', 'sell'],
      ['asset', 'rgb:ABCDEFGH-1234'], ['amount', '100'], ['price', '1000'], ['expiry', String(createdAt + 86_400)]],
  };
}
const presenceEvent = (createdAt: number, pubkey = MAKER) => ({
  id: `p${createdAt}`, pubkey, kind: 10053, created_at: createdAt, sig: '', content: '{"open_offers":1}',
  tags: [['t', 'rgb-psbt-v1'], ['network', 'signetcustom']],
});

afterEach(() => { vi.useRealTimers(); mock.subs.length = 0; });

describe('liveness rule', () => {
  const NOW = 1_800_000_000;
  it('takes the later of presence and the offer time', () => {
    expect(lastSeen(NOW - 1800, NOW - 30, NOW)).toBe(NOW - 30);
    expect(lastSeen(NOW - 30, undefined, NOW)).toBe(NOW - 30); // a maker that still republishes every minute
  });
  it('online, away after 3 minutes, gone after 15', () => {
    expect(liveness(NOW - AWAY_AFTER, NOW)).toBe('online');
    expect(liveness(NOW - AWAY_AFTER - 1, NOW)).toBe('away');
    expect(liveness(NOW - GONE_AFTER - 1, NOW)).toBe('gone');
  });
  it('does not believe timestamps far ahead of the clock', () => {
    expect(lastSeen(NOW - 1800, NOW + 3600, NOW)).toBe(NOW - 1800);
    expect(lastSeen(NOW - 1800, NOW + 60, NOW)).toBe(NOW);
  });
});

describe('the book with presence', () => {
  it('keeps a 30-minute-old offer online while presence is fresh, and drops it when both go stale', () => {
    vi.useFakeTimers();
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useRgbP2pBook('wss://relay.test'));
    act(() => mock.subs[0]!.handlers.onevent(offerEvent('o1', now - 1800)));
    // presence is asked for the makers in the book only
    const presenceSub = mock.subs.find((s) => s.filter.kinds?.[0] === 10053)!;
    expect(presenceSub.filter.authors).toEqual([MAKER]);
    expect(presenceSub.filter.until).toBeUndefined();
    expect(result.current.offers).toHaveLength(0); // 30 minutes without a sign: gone
    act(() => presenceSub.handlers.onevent(presenceEvent(now - 20)));
    expect(result.current.offers).toHaveLength(1);
    expect(result.current.offers[0]!.away).toBe(false);
    // ten minutes later, no new presence: away, still shown
    act(() => vi.advanceTimersByTime(600_000));
    expect(result.current.offers[0]!.away).toBe(true);
    // and past fifteen: gone
    act(() => vi.advanceTimersByTime(330_000));
    expect(result.current.offers).toHaveLength(0);
  });

  it('ignores presence from keys outside the book', () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useRgbP2pBook('wss://relay.test'));
    act(() => mock.subs[0]!.handlers.onevent(offerEvent('o1', now - 1800)));
    const presenceSub = mock.subs.find((s) => s.filter.kinds?.[0] === 10053)!;
    act(() => presenceSub.handlers.onevent(presenceEvent(now - 10, OTHER)));
    expect(result.current.offers).toHaveLength(0);
  });

  it('still works for makers that republish every offer every minute', () => {
    const now = Math.floor(Date.now() / 1000);
    const { result } = renderHook(() => useRgbP2pBook('wss://relay.test'));
    act(() => mock.subs[0]!.handlers.onevent(offerEvent('o1', now - 40)));
    expect(result.current.offers).toHaveLength(1);
    expect(result.current.offers[0]!.away).toBe(false);
  });
});
