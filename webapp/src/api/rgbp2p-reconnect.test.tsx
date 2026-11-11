import { act, renderHook } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
const mock = vi.hoisted(() => ({ subscriptions: [] as any[], close: vi.fn() }));
vi.mock('nostr-tools', () => ({ SimplePool: class {
  subscribeMany(_relays: unknown, _filter: unknown, handlers: unknown) {
    mock.subscriptions.push(handlers);
    return { close: mock.close };
  }
  close() {}
} }));
import { useRgbP2pBook } from './rgbp2p';
afterEach(() => { vi.useRealTimers(); mock.subscriptions.length = 0; });
it('retries an initial relay timeout and cancels retries on unmount', () => {
  vi.useFakeTimers();
  const { result, unmount } = renderHook(() => useRgbP2pBook());
  act(() => mock.subscriptions[0].onclose(['connection timed out']));
  expect(result.current.status).toBe('error');
  act(() => vi.advanceTimersByTime(1000));
  expect(mock.subscriptions).toHaveLength(2);
  act(() => mock.subscriptions[1].oneose());
  expect(result.current.status).toBe('open');
  expect(result.current.error).toBeUndefined();
  act(() => mock.subscriptions[1].onclose(['offline']));
  unmount();
  act(() => vi.advanceTimersByTime(30000));
  expect(mock.subscriptions).toHaveLength(2);
});
