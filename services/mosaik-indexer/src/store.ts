/** SQLite persistence (node:sqlite). One file, written by one indexer process. */

import { DatabaseSync } from 'node:sqlite';
import type { ParsedOffer } from './offers.ts';
import type { SwapMatch, SwapTerms } from './settlement.ts';
import { BUCKET_SECS, type OrderRecord, type SettleState } from './scoring.ts';
import type { NostrEvent } from './nostr.ts';

const SCHEMA = `
CREATE TABLE IF NOT EXISTS orders (
  maker TEXT NOT NULL, order_id TEXT NOT NULL,
  side TEXT NOT NULL, asset_id TEXT NOT NULL, amount INTEGER NOT NULL, price_sat INTEGER NOT NULL,
  expiry INTEGER NOT NULL, first_seen INTEGER NOT NULL, last_seen INTEGER NOT NULL,
  received_at INTEGER NOT NULL,
  status TEXT NOT NULL, taken_at INTEGER, aborted INTEGER NOT NULL DEFAULT 0,
  terms TEXT NOT NULL, txid_hint TEXT,
  settle_state TEXT NOT NULL DEFAULT 'none', settlement TEXT,
  next_check INTEGER, checks INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (maker, order_id)
);
CREATE INDEX IF NOT EXISTS orders_check ON orders (next_check) WHERE next_check IS NOT NULL;
CREATE TABLE IF NOT EXISTS status_log (
  event_id TEXT PRIMARY KEY, maker TEXT NOT NULL, order_id TEXT NOT NULL,
  status TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS presence (maker TEXT NOT NULL, bucket INTEGER NOT NULL, PRIMARY KEY (maker, bucket)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS coverage (bucket INTEGER PRIMARY KEY);
CREATE TABLE IF NOT EXISTS profiles (maker TEXT PRIMARY KEY, created_at INTEGER NOT NULL, event TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS published (kind INTEGER NOT NULL, d TEXT NOT NULL, event TEXT NOT NULL, PRIMARY KEY (kind, d));
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT NOT NULL);
`;

interface OrderRow {
  maker: string;
  order_id: string;
  side: 'sell' | 'buy';
  asset_id: string;
  amount: number;
  price_sat: number;
  expiry: number;
  first_seen: number;
  last_seen: number;
  received_at: number;
  status: string;
  taken_at: number | null;
  aborted: number;
  terms: string;
  txid_hint: string | null;
  settle_state: SettleState;
  settlement: string | null;
  next_check: number | null;
  checks: number;
}

export interface CheckJob {
  maker: string;
  orderId: string;
  status: string;
  expiry: number;
  takenAt: number | null;
  lastSeen: number;
  terms: SwapTerms;
  txidHint: string | null;
  settleState: SettleState;
  checks: number;
}

export class Store {
  readonly db: DatabaseSync;

  constructor(path: string) {
    this.db = new DatabaseSync(path);
    this.db.exec('PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;');
    this.db.exec(SCHEMA);
  }

  close() {
    this.db.close();
  }

  getMeta(k: string): string | undefined {
    const r = this.db.prepare('SELECT v FROM meta WHERE k = ?').get(k) as { v: string } | undefined;
    return r?.v;
  }

  setMeta(k: string, v: string) {
    this.db.prepare('INSERT INTO meta (k, v) VALUES (?, ?) ON CONFLICT (k) DO UPDATE SET v = excluded.v').run(k, v);
  }

  markPresent(maker: string, at: number) {
    this.db.prepare('INSERT OR IGNORE INTO presence (maker, bucket) VALUES (?, ?)').run(maker, Math.floor(at / BUCKET_SECS));
  }

  markCovered(at: number) {
    this.db.prepare('INSERT OR IGNORE INTO coverage (bucket) VALUES (?)').run(Math.floor(at / BUCKET_SECS));
  }

  presenceOf(maker: string, fromBucket: number): number[] {
    return (this.db.prepare('SELECT bucket FROM presence WHERE maker = ? AND bucket >= ?').all(maker, fromBucket) as { bucket: number }[]).map(
      (r) => r.bucket,
    );
  }

  coverage(fromBucket: number): number[] {
    return (this.db.prepare('SELECT bucket FROM coverage WHERE bucket >= ?').all(fromBucket) as { bucket: number }[]).map((r) => r.bucket);
  }

  /**
   * Fold one offer event into the order it belongs to. Returns false when the event is older than
   * what is already known (a relay replaying history, or an out-of-order delivery).
   */
  applyOffer(o: ParsedOffer, now: number): boolean {
    const row = this.db.prepare('SELECT * FROM orders WHERE maker = ? AND order_id = ?').get(o.maker, o.orderId) as OrderRow | undefined;
    const terms: SwapTerms = {
      side: o.side,
      priceSat: o.priceSat,
      makerBtcAddress: o.makerBtcAddress,
      makerRgbScriptHex: o.makerRgbScriptHex,
      rgbOutputSat: o.rgbOutputSat,
      networkFeeSat: o.networkFeeSat,
      platformFeeSat: o.platformFeeSat,
      feeScriptHex: o.feeScriptHex,
    };
    if (row && o.status !== row.status) {
      this.db
        .prepare('INSERT OR IGNORE INTO status_log (event_id, maker, order_id, status, created_at) VALUES (?, ?, ?, ?, ?)')
        .run(o.eventId, o.maker, o.orderId, o.status, o.createdAt);
    }
    if (!row) {
      const takenAt = o.status === 'taken' ? o.createdAt : null;
      this.db
        .prepare(
          `INSERT INTO orders (maker, order_id, side, asset_id, amount, price_sat, expiry, first_seen, last_seen, received_at, status,
             taken_at, terms, txid_hint, settle_state, next_check)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
        )
        .run(
          o.maker, o.orderId, o.side, o.assetId, o.amount, o.priceSat, o.expiry, o.createdAt, o.createdAt, now, o.status, takenAt,
          JSON.stringify(terms), o.txidHint,
          o.status === 'taken' ? 'pending' : 'none',
          o.status === 'taken' ? now : o.status === 'active' ? Math.max(now, o.expiry) + 600 : null,
        );
      if (o.status !== 'active') {
        this.db
          .prepare('INSERT OR IGNORE INTO status_log (event_id, maker, order_id, status, created_at) VALUES (?, ?, ?, ?, ?)')
          .run(o.eventId, o.maker, o.orderId, o.status, o.createdAt);
      }
      return true;
    }
    if (o.createdAt < row.last_seen) return false;
    // the terms of an order never change (rgb-lib ties them to the swap id); keep the first ones
    let takenAt = row.taken_at;
    let aborted = row.aborted;
    let settleState = row.settle_state;
    let nextCheck = row.next_check;
    if (o.status === 'taken' && takenAt === null) {
      takenAt = o.createdAt;
      if (settleState === 'none') settleState = 'pending';
      nextCheck = now;
    }
    // p2pd never lists a taken order again (it relists the terms under a new `d`): `taken` followed
    // by `active` or `cancelled` is a negotiation that did not finish
    if (o.status === 'cancelled' || (o.status === 'active' && takenAt !== null)) {
      if (takenAt !== null && settleState !== 'settled') {
        aborted = 1;
        settleState = 'none';
      }
      if (settleState !== 'settled') nextCheck = null;
    }
    if (o.status === 'active' && settleState === 'none') nextCheck = Math.max(now, o.expiry) + 600;
    if (o.txidHint && settleState !== 'settled') nextCheck = now;
    this.db
      .prepare(
        `UPDATE orders SET last_seen = ?, status = ?, expiry = ?, taken_at = ?, aborted = ?, settle_state = ?, next_check = ?,
           txid_hint = COALESCE(?, txid_hint) WHERE maker = ? AND order_id = ?`,
      )
      .run(o.createdAt, o.status, o.expiry, takenAt, aborted, settleState, nextCheck, o.txidHint, o.maker, o.orderId);
    return true;
  }

  dueChecks(now: number, limit: number): CheckJob[] {
    const rows = this.db
      .prepare('SELECT * FROM orders WHERE next_check IS NOT NULL AND next_check <= ? ORDER BY next_check LIMIT ?')
      .all(now, limit) as unknown as OrderRow[];
    return rows.map((r) => ({
      maker: r.maker,
      orderId: r.order_id,
      status: r.status,
      expiry: r.expiry,
      takenAt: r.taken_at,
      lastSeen: r.last_seen,
      terms: JSON.parse(r.terms) as SwapTerms,
      txidHint: r.txid_hint,
      settleState: r.settle_state,
      checks: r.checks,
    }));
  }

  recordCheck(maker: string, orderId: string, state: SettleState, settlement: SwapMatch | null, nextCheck: number | null, counted = true) {
    this.db
      .prepare('UPDATE orders SET settle_state = ?, settlement = COALESCE(?, settlement), next_check = ?, checks = checks + ? WHERE maker = ? AND order_id = ?')
      .run(state, settlement ? JSON.stringify(settlement) : null, nextCheck, counted ? 1 : 0, maker, orderId);
  }

  makers(): string[] {
    return (this.db.prepare('SELECT DISTINCT maker FROM orders ORDER BY maker').all() as { maker: string }[]).map((r) => r.maker);
  }

  /** Orders that matter to scores: anything taken or settled. Open and cancelled offers do not. */
  scoredOrders(): OrderRecord[] {
    const rows = this.db
      .prepare("SELECT * FROM orders WHERE taken_at IS NOT NULL OR settle_state = 'settled'")
      .all() as unknown as OrderRow[];
    return rows.map((r) => ({
      maker: r.maker,
      orderId: r.order_id,
      side: r.side,
      assetId: r.asset_id,
      amount: r.amount,
      priceSat: r.price_sat,
      firstSeen: r.first_seen,
      lastSeen: r.last_seen,
      receivedAt: r.received_at,
      takenAt: r.taken_at,
      aborted: r.aborted === 1,
      settleState: r.settle_state,
      settlement: r.settlement ? (JSON.parse(r.settlement) as SwapMatch) : null,
    }));
  }

  firstSeen(maker: string): number | null {
    const r = this.db.prepare('SELECT MIN(first_seen) AS t FROM orders WHERE maker = ?').get(maker) as { t: number | null };
    return r.t;
  }

  saveProfile(e: NostrEvent) {
    this.db
      .prepare(
        'INSERT INTO profiles (maker, created_at, event) VALUES (?, ?, ?) ON CONFLICT (maker) DO UPDATE SET created_at = excluded.created_at, event = excluded.event WHERE excluded.created_at > profiles.created_at',
      )
      .run(e.pubkey, e.created_at, JSON.stringify(e));
  }

  profile(maker: string): NostrEvent | null {
    const r = this.db.prepare('SELECT event FROM profiles WHERE maker = ?').get(maker) as { event: string } | undefined;
    return r ? (JSON.parse(r.event) as NostrEvent) : null;
  }

  savePublished(e: NostrEvent) {
    const d = e.tags.find((t) => t[0] === 'd')?.[1] ?? '';
    this.db
      .prepare('INSERT INTO published (kind, d, event) VALUES (?, ?, ?) ON CONFLICT (kind, d) DO UPDATE SET event = excluded.event')
      .run(e.kind, d, JSON.stringify(e));
  }

  published(kind: number, d?: string): NostrEvent[] {
    const rows = (
      d === undefined
        ? this.db.prepare('SELECT event FROM published WHERE kind = ? ORDER BY d').all(kind)
        : this.db.prepare('SELECT event FROM published WHERE kind = ? AND d = ?').all(kind, d)
    ) as { event: string }[];
    return rows.map((r) => JSON.parse(r.event) as NostrEvent);
  }

  /** Forget presence and coverage older than the window; orders stay (they are small). */
  prune(beforeBucket: number) {
    this.db.prepare('DELETE FROM presence WHERE bucket < ?').run(beforeBucket);
    this.db.prepare('DELETE FROM coverage WHERE bucket < ?').run(beforeBucket);
  }
}
