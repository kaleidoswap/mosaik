/**
 * Swaps this page started. The wallet runs each swap (and keeps running it if the page closes); this
 * file remembers their ids in the browser, asks the wallet for their state again after a reload, and
 * follows the ones still moving. Nothing here signs or spends anything.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { RgbOffer } from './rgbp2p';
import type { SwapState, SwapStatus, TakeTerms } from './rgbWallet';
import { useWebRgb } from './webrgb';

/** The order as the user saw it when they took it. */
export interface OrderSnapshot {
  key: string;
  orderId: string;
  maker: string;
  side: 'sell' | 'buy';
  assetId: string;
  amount: number;
  priceSat: number;
  expiry: number;
  network: string;
  ticker?: string;
  name?: string;
  precision: number;
  platformFeeSat?: number;
  networkFeeSat?: number;
  rgbOutputSat?: number;
}

export interface SwapRecord {
  swapId: string;
  order: OrderSnapshot;
  /** ms, when the wallet accepted the request. */
  startedAt: number;
  updatedAt: number;
  state: SwapState;
  txid?: string;
  confirmations?: number;
  error?: { code: string; message: string };
  /** Balances when the swap was requested, to show what changed. */
  before?: { sat: number | null; asset: number | null };
}

export const snapshotOf = (o: RgbOffer): OrderSnapshot => ({
  key: o.key,
  orderId: o.orderId,
  maker: o.maker,
  side: o.side,
  assetId: o.assetId,
  amount: o.amount,
  priceSat: o.priceSat,
  expiry: o.expiry,
  network: o.network,
  ticker: o.ticker,
  name: o.name,
  precision: o.precision,
  platformFeeSat: o.platformFeeSat,
  networkFeeSat: o.networkFeeSat,
  rgbOutputSat: o.rgbOutputSat,
});

export const termsOf = (o: OrderSnapshot): TakeTerms => ({
  orderId: o.orderId,
  maker: o.maker,
  assetId: o.assetId,
  side: o.side,
  amount: o.amount,
  priceSat: o.priceSat,
});

export const isTerminal = (s: SwapState): boolean => s === 'settled' || s === 'failed' || s === 'expired';

/** How long the wallet waits for the maker before it gives up, as the contract proposes. */
export const MAKER_WAIT_SECONDS = 120;

const KEY = 'mosaik.p2p.swaps.v1';
const MAX = 30;
const STATES: SwapState[] = ['awaiting_maker', 'signing', 'broadcast', 'settled', 'failed', 'expired'];

const isRecord = (v: unknown): v is SwapRecord => {
  const r = v as SwapRecord | null;
  return (
    !!r &&
    typeof r.swapId === 'string' &&
    typeof r.startedAt === 'number' &&
    STATES.includes(r.state) &&
    !!r.order &&
    typeof r.order.assetId === 'string' &&
    typeof r.order.amount === 'number' &&
    typeof r.order.priceSat === 'number'
  );
};

const load = (): SwapRecord[] => {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? '[]') as unknown;
    return Array.isArray(raw) ? raw.filter(isRecord).slice(0, MAX) : [];
  } catch {
    return [];
  }
};
const save = (list: SwapRecord[]) => {
  try {
    localStorage.setItem(KEY, JSON.stringify(list.slice(0, MAX)));
  } catch {
    /* private mode or full: the list just does not survive a reload */
  }
};

interface SwapsApi {
  swaps: SwapRecord[];
  /** Ask the wallet to take an order. Resolves once the wallet accepted the request (after the user approved). */
  start: (order: OrderSnapshot) => Promise<SwapRecord>;
  clearFinished: () => void;
}

const Ctx = createContext<SwapsApi | null>(null);

export function SwapsProvider({ children }: { children: ReactNode }) {
  const wallet = useWebRgb();
  const [swaps, setSwaps] = useState<SwapRecord[]>(load);
  const subs = useRef(new Map<string, () => void>());
  const starting = useRef(new Map<string, Promise<SwapRecord>>());
  const currentSwaps = useRef(swaps);
  currentSwaps.current = swaps;
  const provider = wallet.provider;
  const refresh = wallet.refresh;
  const connected = wallet.status === 'connected';
  // `start` can run right after the wallet connected, in the same render that began it
  const latest = useRef(wallet);
  latest.current = wallet;

  useEffect(() => save(swaps), [swaps]);

  const apply = useCallback(
    (swapId: string, s: SwapStatus) => {
      if (!s || !STATES.includes(s.state)) return; // a wallet that does not know the swap, or says something new
      setSwaps((prev) =>
        prev.map((r) =>
          r.swapId !== swapId
            ? r
            : {
                ...r,
                state: s.state,
                txid: s.txid ?? r.txid,
                confirmations: s.confirmations ?? r.confirmations,
                error: s.error ?? (s.state === 'failed' || s.state === 'expired' ? r.error : undefined),
                updatedAt: Date.now(),
              },
        ),
      );
      if (s.state === 'settled') {
        void refresh();
        window.setTimeout(() => void refresh(), 6_000);
      }
    },
    [refresh],
  );

  const follow = useCallback(
    (swapId: string) => {
      const p2p = provider?.p2pSwap;
      if (!p2p || subs.current.has(swapId)) return;
      let off: () => void = () => {};
      try {
        off = p2p.onStatus(swapId, (s) => apply(swapId, s));
      } catch {
        /* polling below still covers it */
      }
      subs.current.set(swapId, off);
      p2p.status(swapId).then((s) => apply(swapId, s)).catch(() => {});
    },
    [provider, apply],
  );

  // Re-attach to every swap that was still moving when the page was last open, and stop following finished ones.
  useEffect(() => {
    if (!connected || !provider?.p2pSwap) return;
    for (const r of swaps) {
      if (!isTerminal(r.state)) follow(r.swapId);
      else {
        subs.current.get(r.swapId)?.();
        subs.current.delete(r.swapId);
      }
    }
  }, [connected, provider, swaps, follow]);

  // The event channel only reaches the page that is open; a slow poll covers a missed one.
  const moving = swaps.some((r) => !isTerminal(r.state));
  useEffect(() => {
    if (!connected || !moving || !provider?.p2pSwap) return;
    const p2p = provider.p2pSwap;
    const t = window.setInterval(() => {
      for (const r of swaps) if (!isTerminal(r.state)) p2p.status(r.swapId).then((s) => apply(r.swapId, s)).catch(() => {});
    }, 4_000);
    return () => window.clearInterval(t);
  }, [connected, moving, provider, swaps, apply]);

  useEffect(
    () => () => {
      subs.current.forEach((off) => off());
      subs.current.clear();
    },
    [],
  );

  const start = useCallback(
    async (order: OrderSnapshot): Promise<SwapRecord> => {
      const key = `${order.maker}:${order.orderId}`;
      const pending = starting.current.get(key);
      if (pending) return pending;
      const existing = currentSwaps.current.find((r) => r.order.maker === order.maker && r.order.orderId === order.orderId && (!isTerminal(r.state) || r.state === 'settled'));
      if (existing) return existing;
      if (order.expiry <= Date.now() / 1000) throw new Error('expired: This offer has expired. Choose a current offer.');
      const work = (async () => {
      const p2p = provider?.p2pSwap;
      if (!provider || !p2p) throw new Error('The KaleidoSwap extension is needed to take an order');
      await provider.enable?.();
      const w = latest.current;
      const before = {
        sat: w.balance?.spendable ?? null,
        asset: w.balance ? (w.assets.find((a) => a.id === order.assetId)?.balance ?? 0) : null,
      };
      const { swapId } = await p2p.take(termsOf(order));
      const now = Date.now();
      const rec: SwapRecord = { swapId, order, startedAt: now, updatedAt: now, state: 'awaiting_maker', before };
      setSwaps((prev) => [rec, ...prev.filter((r) => r.swapId !== swapId)].slice(0, MAX));
      return rec;
      })();
      starting.current.set(key, work);
      try { return await work; }
      finally { starting.current.delete(key); }
    },
    [provider],
  );

  const clearFinished = useCallback(() => setSwaps((prev) => prev.filter((r) => !isTerminal(r.state))), []);

  const value = useMemo(() => ({ swaps, start, clearFinished }), [swaps, start, clearFinished]);
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useSwaps(): SwapsApi {
  const v = useContext(Ctx);
  if (!v) throw new Error('useSwaps outside SwapsProvider');
  return v;
}
