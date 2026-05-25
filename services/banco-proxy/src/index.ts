/**
 * banco-proxy — Mosaik's HTTP gateway to @arkade-os/banco.
 *
 * Arkade has no Rust SDK; the Mosaik Rust workspace drives banco swaps via
 * this small Express service. The endpoint shape is documented in the
 * mosaik-banco Rust client (crates/mosaik-banco/src/client.rs) and the
 * README in this directory — keep all three in sync.
 *
 * Until @arkade-os/banco is on a stable npm channel, the SDK calls are
 * stubbed in src/services/banco.ts. The HTTP contract is final; only the
 * service-layer body changes when we wire the real SDK in.
 */

import express from 'express';
import { pinoHttp } from 'pino-http';
import { logger } from './logger.js';
import { createAuthRoutes } from './routes/auth.js';
import { createBancoRoutes } from './routes/banco.js';

const PORT = Number(process.env.BANCO_PROXY_PORT ?? 3001);

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '1mb' }));
app.use(pinoHttp({ logger }));

app.get('/healthz', (_req, res) => res.json({ status: 'ok' }));

app.use('/api/v1/auth', createAuthRoutes());
app.use('/api/v1/banco', createBancoRoutes());

// Error fall-through.
app.use(
  (err: Error, _req: express.Request, res: express.Response, _next: express.NextFunction) => {
    logger.error({ err }, 'unhandled error');
    res.status(500).json({ error: err.message });
  },
);

app.listen(PORT, () => {
  logger.info(`banco-proxy listening on http://0.0.0.0:${PORT}`);
});
