/** Read-only HTTP API. Everything it serves is also verifiable from the signed events it returns. */

import { createServer, type Server } from 'node:http';
import type { Indexer } from './indexer.ts';
import type { Store } from './store.ts';
import type { Signer } from './nostr.ts';
import { CATALOG_KIND, REPUTATION_KIND } from './events.ts';
import type { Config } from './config.ts';

export function startHttp(indexer: Indexer, store: Store, signer: Signer, cfg: Config): Server {
  const server = createServer((req, res) => {
    const send = (status: number, body: unknown) => {
      res.writeHead(status, {
        'content-type': 'application/json; charset=utf-8',
        'access-control-allow-origin': '*',
        'cache-control': 'no-store',
      });
      res.end(JSON.stringify(body));
    };
    if (req.method === 'OPTIONS') {
      res.writeHead(204, { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'GET' });
      return res.end();
    }
    if (req.method !== 'GET') return send(405, { error: 'read only' });
    const url = new URL(req.url ?? '/', 'http://x');
    const path = url.pathname.replace(/\/+$/, '') || '/';
    const now = Math.floor(Date.now() / 1000);

    if (path === '/health') return send(200, { ok: true, relay: indexer.relay.live ? 'subscribed' : 'down', network: cfg.network });
    if (path === '/v1/indexer')
      return send(200, {
        pubkey: signer.pubkey,
        npub: signer.npub,
        network: cfg.network,
        relay: cfg.relayUrl,
        esplora: cfg.esploraUrl,
        publishing: cfg.publish,
        kinds: { reputation: REPUTATION_KIND, catalog: CATALOG_KIND },
        thresholds: cfg.thresholds,
        devKey: true,
      });
    if (path === '/v1/makers') {
      const scores = indexer.scores(now).map(({ settlements: _s, ...rest }) => rest);
      return send(200, { computedAt: now, makers: scores });
    }
    const m = path.match(/^\/v1\/makers\/([0-9a-f]{64})(\/event)?$/);
    if (m) {
      const maker = m[1]!;
      if (m[2]) {
        const e = store.published(REPUTATION_KIND, maker)[0];
        return e ? send(200, e) : send(404, { error: 'no signed score for this maker yet' });
      }
      const score = indexer.scores(now).find((s) => s.maker === maker);
      if (!score) return send(404, { error: 'unknown maker' });
      return send(200, { computedAt: now, score, profile: store.profile(maker) });
    }
    if (path === '/v1/events/reputation') return send(200, store.published(REPUTATION_KIND));
    if (path === '/v1/assets') {
      const e = store.published(CATALOG_KIND)[0];
      return e ? send(200, { event: e, catalog: JSON.parse(e.content) }) : send(404, { error: 'curated list not signed yet' });
    }
    return send(404, { error: 'not found' });
  });
  server.listen(cfg.httpPort, cfg.httpHost);
  return server;
}
