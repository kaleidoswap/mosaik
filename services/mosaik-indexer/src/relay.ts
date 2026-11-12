/**
 * A small, reconnecting relay client on Node's own WebSocket. One socket; subscriptions are
 * re-sent after each reconnect with `since` moved up to what was already seen, so a gap in the
 * connection is filled from the relay (as far as the relay still holds those events).
 */

import type { NostrEvent } from './nostr.ts';
import { log } from './log.ts';

export interface Filter {
  kinds?: number[];
  authors?: string[];
  since?: number;
  until?: number;
  limit?: number;
  [tag: `#${string}`]: string[] | undefined;
}

interface Sub {
  id: string;
  /** Called on every (re)connect to build the filter, so `since` can follow progress. */
  filter: () => Filter;
  onEvent: (e: NostrEvent) => void;
  onEose?: () => void;
}

export class Relay {
  readonly url: string;
  private ws: WebSocket | null = null;
  private subs = new Map<string, Sub>();
  private pendingOk = new Map<string, (r: { ok: boolean; message: string }) => void>();
  private stopped = false;
  private attempt = 0;
  private openListeners: (() => void)[] = [];
  /** True while the socket is open and the subscriptions have been sent. */
  live = false;
  onState: (live: boolean) => void = () => {};

  constructor(url: string) {
    this.url = url;
  }

  start() {
    this.stopped = false;
    this.connect();
  }

  stop() {
    this.stopped = true;
    this.ws?.close();
  }

  subscribe(sub: Sub) {
    this.subs.set(sub.id, sub);
    if (this.live) this.send(['REQ', sub.id, sub.filter()]);
  }

  /** Publish and wait for the relay's OK. */
  async publish(e: NostrEvent, timeoutMs = 10_000): Promise<{ ok: boolean; message: string }> {
    await this.waitOpen(timeoutMs);
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pendingOk.delete(e.id);
        resolve({ ok: false, message: 'timeout waiting for OK' });
      }, timeoutMs);
      this.pendingOk.set(e.id, (r) => {
        clearTimeout(timer);
        resolve(r);
      });
      this.send(['EVENT', e]);
    });
  }

  private waitOpen(timeoutMs: number): Promise<void> {
    if (this.live) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => reject(new Error(`relay ${this.url} not connected`)), timeoutMs);
      this.openListeners.push(() => {
        clearTimeout(t);
        resolve();
      });
    });
  }

  private send(msg: unknown[]) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(msg));
  }

  private setLive(v: boolean) {
    if (this.live === v) return;
    this.live = v;
    this.onState(v);
  }

  private connect() {
    if (this.stopped) return;
    const ws = new WebSocket(this.url);
    this.ws = ws;
    ws.onopen = () => {
      this.attempt = 0;
      log.info('relay connected', { url: this.url });
      for (const s of this.subs.values()) this.send(['REQ', s.id, s.filter()]);
      this.setLive(true);
      for (const l of this.openListeners.splice(0)) l();
    };
    ws.onmessage = (m) => {
      let msg: unknown[];
      try {
        msg = JSON.parse(String(m.data));
      } catch {
        return;
      }
      if (!Array.isArray(msg)) return;
      const [type, a, b, c] = msg as [string, unknown, unknown, unknown];
      if (type === 'EVENT' && typeof a === 'string') this.subs.get(a)?.onEvent(b as NostrEvent);
      else if (type === 'EOSE' && typeof a === 'string') this.subs.get(a)?.onEose?.();
      else if (type === 'OK' && typeof a === 'string') {
        this.pendingOk.get(a)?.({ ok: b === true, message: String(c ?? '') });
        this.pendingOk.delete(a);
      } else if (type === 'CLOSED' && typeof a === 'string') {
        log.warn('relay closed a subscription', { url: this.url, sub: a, reason: String(b ?? '') });
      } else if (type === 'NOTICE') log.warn('relay notice', { url: this.url, notice: String(a) });
      // AUTH: reads on this relay need none, and the indexer never authenticates
    };
    ws.onclose = () => {
      this.setLive(false);
      if (this.stopped) return;
      const delay = Math.min(60_000, 1000 * 2 ** Math.min(this.attempt++, 6));
      log.warn('relay disconnected, retrying', { url: this.url, inMs: delay });
      setTimeout(() => this.connect(), delay);
    };
    ws.onerror = () => {
      /* onclose follows */
    };
  }
}
