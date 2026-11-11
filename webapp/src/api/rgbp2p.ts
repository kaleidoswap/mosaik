/**
 * The RGB peer-to-peer order book, read from the public Nostr relay.
 *
 * Makers publish each order as an addressable event (kind 30053, `t` = `rgb-psbt-v1`). The facts a
 * book needs are public tags — side, asset, amount, price, expiry, network, status — so this reads
 * them straight from the tags and never has to trust or parse the offer inside the content. The
 * ticker and precision are the maker's own words (in the content), shown as a hint only: the asset
 * id is the identity.
 *
 * Read only. Taking an order needs a wallet that signs, which is the browser extension's job.
 */

import { useEffect, useMemo, useState } from 'react';
import { SimplePool, type Event } from 'nostr-tools';
import { safeLabel } from '../lib/text';

export const RGB_OFFER_KIND = 30053;
export const RGB_PROTOCOL_TAG = 'rgb-psbt-v1';

const DEFAULT_RELAY = import.meta.env.VITE_RGB_RELAY_URL ?? 'wss://relay.kaleidoswap.com';
const MAX_EVENTS = 500;

export type RgbSide = 'sell' | 'buy';

export interface RgbOffer {
  /** `pubkey:d`, the address of the order. */
  key: string;
  orderId: string;
  maker: string;
  side: RgbSide;
  assetId: string;
  /** Asset units, in the asset's smallest unit. */
  amount: number;
  /** The BTC leg, in sats. */
  priceSat: number;
  expiry: number;
  network: string;
  createdAt: number;
  /** What the maker says the asset is. A hint: only `assetId` is an identity. */
  ticker?: string;
  name?: string;
  precision: number;
  /** Fees the offer itself carries, in sats. The taker funds them; undefined when the offer does not say. */
  platformFeeSat?: number;
  networkFeeSat?: number;
  /** Sats the RGB output carries. */
  rgbOutputSat?: number;
}

const tag = (e: Event, name: string): string | undefined => e.tags.find((t) => t[0] === name)?.[1];

const whole = (s: string | undefined): number | undefined => {
  if (s === undefined || !/^\d{1,16}$/.test(s)) return undefined;
  return Number(s);
};

/** An order a maker reports as taken. Makers say so themselves: nothing here proves it settled. */
export interface RgbFill {
  key: string;
  maker: string;
  side: RgbSide;
  assetId: string;
  amount: number;
  priceSat: number;
  network: string;
  precision: number;
  ticker?: string;
  /** Seconds, when the maker published the status. */
  at: number;
}

export function parseRgbFill(e: Event): RgbFill | null {
  if (e.kind !== RGB_OFFER_KIND || tag(e, 't') !== RGB_PROTOCOL_TAG || tag(e, 'status') !== 'taken') return null;
  const side = tag(e, 'side');
  const orderId = tag(e, 'd');
  const assetId = tag(e, 'asset');
  const amount = whole(tag(e, 'amount'));
  const priceSat = whole(tag(e, 'price'));
  const network = tag(e, 'network');
  if ((side !== 'sell' && side !== 'buy') || !orderId || !assetId || !amount || !priceSat || !network) return null;
  if (!/^rgb:[A-Za-z0-9~_:-]{8,120}$/.test(assetId)) return null;
  let precision = 0;
  let ticker: string | undefined;
  try {
    const meta = (JSON.parse(e.content) as { asset?: { ticker?: unknown; precision?: unknown } }).asset;
    ticker = safeLabel(meta?.ticker, 12);
    if (typeof meta?.precision === 'number' && meta.precision >= 0 && meta.precision <= 18) precision = meta.precision;
  } catch {
    /* the tags are enough */
  }
  return { key: `${e.pubkey}:${orderId}`, maker: e.pubkey, side, assetId, amount, priceSat, network, precision, ticker, at: e.created_at };
}

/** Sats per whole asset unit of a fill. */
export function fillUnitPrice(f: Pick<RgbFill, 'amount' | 'precision' | 'priceSat'>): number | null {
  const units = f.amount / 10 ** f.precision;
  return units > 0 ? f.priceSat / units : null;
}

/** An event as an order, or null when it is not a live, well-formed one. */
export function parseRgbOffer(e: Event, now = Math.floor(Date.now() / 1000)): RgbOffer | null {
  if (e.kind !== RGB_OFFER_KIND || tag(e, 't') !== RGB_PROTOCOL_TAG) return null;
  if (tag(e, 'status') !== 'active') return null;
  const side = tag(e, 'side');
  if (side !== 'sell' && side !== 'buy') return null;
  const orderId = tag(e, 'd');
  const assetId = tag(e, 'asset');
  const amount = whole(tag(e, 'amount'));
  const priceSat = whole(tag(e, 'price'));
  const expiry = whole(tag(e, 'expiry'));
  const network = tag(e, 'network');
  if (!orderId || !assetId || !network) return null;
  if (!amount || !priceSat || !expiry || expiry <= now) return null;
  if (!/^rgb:[A-Za-z0-9~_:-]{8,120}$/.test(assetId)) return null;

  let ticker: string | undefined;
  let name: string | undefined;
  let precision = 0;
  let platformFeeSat: number | undefined;
  let networkFeeSat: number | undefined;
  let rgbOutputSat: number | undefined;
  try {
    const content = JSON.parse(e.content) as {
      asset?: { ticker?: unknown; name?: unknown; precision?: unknown };
      offer?: { platformFeeSat?: unknown; networkFeeSat?: unknown; rgbOutputSat?: unknown };
    };
    const sat = (v: unknown): number | undefined =>
      typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 ? v : undefined;
    platformFeeSat = sat(content.offer?.platformFeeSat);
    networkFeeSat = sat(content.offer?.networkFeeSat);
    rgbOutputSat = sat(content.offer?.rgbOutputSat);
    const meta = content.asset;
    ticker = safeLabel(meta?.ticker, 12);
    name = safeLabel(meta?.name, 40);
    if (typeof meta?.precision === 'number' && meta.precision >= 0 && meta.precision <= 18) precision = meta.precision;
  } catch {
    /* the tags are enough */
  }

  return {
    key: `${e.pubkey}:${orderId}`,
    orderId,
    maker: e.pubkey,
    side,
    assetId,
    amount,
    priceSat,
    expiry,
    network,
    createdAt: e.created_at,
    ticker,
    name,
    precision,
    platformFeeSat,
    networkFeeSat,
    rgbOutputSat,
  };
}

/** Sats per whole asset unit, or null when it cannot be told. */
export function unitPriceSat(o: RgbOffer): number | null {
  const units = o.amount / 10 ** o.precision;
  return units > 0 ? o.priceSat / units : null;
}

export interface RgbBookState {
  url: string;
  status: 'connecting' | 'open' | 'closed' | 'error';
  offers: RgbOffer[];
  /** Orders makers report as taken, newest first. Self-reported. */
  fills: RgbFill[];
  error?: string;
}

/** The live book: newest event per order, expired and non-active ones dropped. */
export function useRgbP2pBook(relayUrl: string = DEFAULT_RELAY): RgbBookState {
  const [byKey, setByKey] = useState<Map<string, { offer: RgbOffer | null; fill: RgbFill | null; at: number }>>(new Map());
  const [status, setStatus] = useState<RgbBookState['status']>('connecting');
  const [error, setError] = useState<string>();
  const [retry, setRetry] = useState(0);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    setByKey(new Map());
    setStatus('connecting');
    setError(undefined);
    let stopped = false;
    let reconnect: ReturnType<typeof setTimeout> | undefined;
    const pool = new SimplePool({ enablePing: true, enableReconnect: true });
    const sub = pool.subscribeMany(
      [relayUrl],
      { kinds: [RGB_OFFER_KIND], '#t': [RGB_PROTOCOL_TAG], limit: MAX_EVENTS },
      {
        onevent(e: Event) {
          if (stopped) return;
          // a cancelled or taken order replaces the active one, so keep the newest event either way
          const d = tag(e, 'd');
          if (!d) return;
          const key = `${e.pubkey}:${d}`;
          const offer = parseRgbOffer(e, 0);
          const fill = offer ? null : parseRgbFill(e);
          setStatus('open');
          setError(undefined);
          setByKey((prev) => {
            const old = prev.get(key);
            if (old && old.at >= e.created_at) return prev;
            const next = new Map(prev);
            next.set(key, { offer, fill, at: e.created_at });
            return next;
          });
        },
        oneose() {
          if (stopped) return;
          setStatus('open');
          setError(undefined);
        },
        onclose(reasons: string[]) {
          // closing it ourselves (a new effect run, or leaving the page) is not a relay failure
          if (stopped) return;
          const reason = reasons.filter(Boolean).join('; ');
          setStatus(reason ? 'error' : 'closed');
          setError(reason || undefined);
          // Pool reconnect covers established sockets, but not a failed initial
          // connection/subscription. Recreate the subscription in that case too.
          if (!reconnect) reconnect = setTimeout(() => {
            if (!stopped) setRetry((n) => n + 1);
          }, Math.min(30_000, 1000 * 2 ** Math.min(retry, 5)));
        },
      },
    );
    return () => {
      stopped = true;
      if (reconnect) clearTimeout(reconnect);
      try {
        sub.close();
      } catch {
        /* already closed */
      }
      try {
        pool.close([relayUrl]);
      } catch {
        /* already closed */
      }
    };
  }, [relayUrl, retry]);

  const offers = useMemo(
    () =>
      [...byKey.values()]
        .map((v) => v.offer)
        .filter((o): o is RgbOffer => o !== null && o.expiry > now),
    [byKey, now],
  );

  const fills = useMemo(
    () => [...byKey.values()].map((v) => v.fill).filter((f): f is RgbFill => f !== null).sort((a, b) => b.at - a.at),
    [byKey],
  );

  return { url: relayUrl, status, offers, fills, error };
}
