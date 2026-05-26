/**
 * Live WebSocket subscription to a Mosaik Nostr relay.
 *
 * `useRelayFeed()` returns:
 *   - `status`        — 'connecting' | 'open' | 'closed' | 'error'
 *   - `events`        — most-recent-first, capped at `MAX_EVENTS`
 *   - `url`           — the relay URL the hook connected to
 *
 * Only the three kinds the Mosaik protocol defines are accepted (30050 offer,
 * 30051 maker profile, 30052 cross-chain swap envelope). Anything else from a
 * (hypothetically misconfigured) relay is dropped client-side.
 *
 * This is a *complement* to `useApiState()` polling, not a replacement: the
 * /api/state endpoint is the authoritative single source of truth for the
 * Liquid chain. Relay events surface what *other* makers / takers are
 * publishing into the discovery layer.
 */

import { useEffect, useRef, useState } from 'react';
import { SimplePool, type Event, type Filter } from 'nostr-tools';

const DEFAULT_RELAY = import.meta.env.VITE_RELAY_URL ?? 'ws://127.0.0.1:7777';
const KINDS = [30050, 30051, 30052] as const;
const MAX_EVENTS = 50;

export type RelayStatus = 'connecting' | 'open' | 'closed' | 'error';

export interface RelayFeedState {
  url: string;
  status: RelayStatus;
  events: Event[];
  error?: string;
}

export function useRelayFeed(relayUrl: string = DEFAULT_RELAY): RelayFeedState {
  const [state, setState] = useState<RelayFeedState>({
    url: relayUrl,
    status: 'connecting',
    events: [],
  });

  // Hold the pool + seen-id set across re-renders without re-subscribing.
  const poolRef = useRef<SimplePool | null>(null);
  const seenIdsRef = useRef<Set<string>>(new Set());

  useEffect(() => {
    seenIdsRef.current = new Set();
    setState((s) => ({ ...s, url: relayUrl, status: 'connecting', events: [] }));

    const pool = new SimplePool();
    poolRef.current = pool;

    const filter: Filter = { kinds: [...KINDS], limit: MAX_EVENTS };

    // SimplePool.subscribeMany returns a SubCloser. We pass our handlers; if
    // the relay is unreachable the `onclose` fires with the reason.
    const sub = pool.subscribeMany([relayUrl], filter, {
      onevent(event: Event) {
        if (!KINDS.includes(event.kind as (typeof KINDS)[number])) return;
        if (seenIdsRef.current.has(event.id)) return;
        seenIdsRef.current.add(event.id);
        setState((s) => ({
          ...s,
          status: 'open',
          events: [event, ...s.events].slice(0, MAX_EVENTS),
        }));
      },
      oneose() {
        // End-of-stored-events — the relay has shipped its history; we're now
        // tailing. Mark connection as open if we haven't already.
        setState((s) => (s.status === 'open' ? s : { ...s, status: 'open' }));
      },
      onclose(reasons: string[]) {
        // nostr-tools fires onclose per relay; treat any non-empty reason as
        // an error (transport / handshake), an empty reason as a clean close.
        const reason = reasons.filter(Boolean).join('; ');
        setState((s) => ({
          ...s,
          status: reason ? 'error' : 'closed',
          error: reason || undefined,
        }));
      },
    });

    return () => {
      try {
        sub.close();
      } catch {
        /* sub already closed */
      }
      try {
        pool.close([relayUrl]);
      } catch {
        /* pool already closed */
      }
      poolRef.current = null;
    };
  }, [relayUrl]);

  return state;
}
