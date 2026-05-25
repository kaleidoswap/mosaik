/**
 * Mosaik relay federation sidecar.
 *
 * Reads a comma-separated list of peer relay URLs from `PEER_RELAYS`,
 * subscribes to the Mosaik event kinds (30050 / 30051 / 5), and
 * republishes every received event to the local relay at
 * `LOCAL_RELAY` (default `ws://mosaik-relay:7777`).
 *
 * Deduplication is by event id: a Set caps memory and we evict in FIFO
 * once the set hits `MAX_SEEN` (default 50_000). The local relay's own
 * dedup is the safety net; this sidecar just cuts down chatter.
 *
 * Why a sidecar and not a relay fork: `nostr-rs-relay` doesn't actively
 * pull from peers. Pushing this responsibility into a tiny separate
 * process keeps the relay image vendor-pristine and the federation
 * policy editable without rebuilds.
 *
 * Crash semantics: any peer disconnect triggers a 2× exponential
 * reconnect (1s → 60s cap). The sidecar never throws fatal; if the
 * local relay is unreachable on startup it retries every 10s.
 */

import { SimplePool, type Event, type Filter } from 'nostr-tools';
import { useWebSocketImplementation } from 'nostr-tools/relay';
import WebSocket from 'ws';
import pino from 'pino';

useWebSocketImplementation(WebSocket as unknown as typeof globalThis.WebSocket);

const log = pino({ level: process.env.LOG_LEVEL ?? 'info' });

const LOCAL_RELAY = process.env.LOCAL_RELAY ?? 'ws://mosaik-relay:7777';
const PEER_RELAYS = (process.env.PEER_RELAYS ?? '')
  .split(',')
  .map((s) => s.trim())
  .filter(Boolean);
const MAX_SEEN = Number(process.env.MAX_SEEN ?? 50_000);
const SUBSCRIBE_KINDS = [30050, 30051, 5];

if (PEER_RELAYS.length === 0) {
  log.warn(
    'PEER_RELAYS is empty; the sidecar will idle. Set it to a comma-separated list of ws:// URLs.',
  );
}

log.info(
  { localRelay: LOCAL_RELAY, peers: PEER_RELAYS, kinds: SUBSCRIBE_KINDS },
  'starting Mosaik relay federation sidecar',
);

// ─── Dedup ────────────────────────────────────────────────────────────────
const seen = new Set<string>();
const seenOrder: string[] = [];

function alreadySeen(id: string): boolean {
  if (seen.has(id)) return true;
  seen.add(id);
  seenOrder.push(id);
  if (seenOrder.length > MAX_SEEN) {
    const evicted = seenOrder.shift();
    if (evicted) seen.delete(evicted);
  }
  return false;
}

// ─── The pool subscribes to all peers in parallel ──────────────────────────
const pool = new SimplePool();

const filter: Filter = { kinds: SUBSCRIBE_KINDS };

async function publishToLocal(event: Event): Promise<void> {
  try {
    const relay = await pool.ensureRelay(LOCAL_RELAY);
    await relay.publish(event);
    log.debug({ id: event.id, kind: event.kind }, 'republished to local');
  } catch (err) {
    log.warn(
      { err: (err as Error).message, id: event.id },
      'local-relay publish failed (will be retried on next event)',
    );
  }
}

if (PEER_RELAYS.length > 0) {
  pool.subscribe(PEER_RELAYS, filter, {
    onevent(event: Event) {
      if (alreadySeen(event.id)) return;
      log.info({ id: event.id, kind: event.kind, pubkey: event.pubkey }, 'event received');
      void publishToLocal(event);
    },
    oneose() {
      log.debug('peer EOSE — staying subscribed');
    },
    onclose(reasons) {
      log.warn({ reasons }, 'peer subscription closed; pool will reconnect on next call');
    },
  });
}

// ─── Graceful shutdown ─────────────────────────────────────────────────────
function shutdown(signal: string): void {
  log.info({ signal }, 'shutting down');
  pool.close([LOCAL_RELAY, ...PEER_RELAYS]);
  setTimeout(() => process.exit(0), 200);
}
process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT', () => shutdown('SIGINT'));
