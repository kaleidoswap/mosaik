/** Configuration from the environment. Mutinynet only: anything else is refused at start. */

import { homedir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MUTINYNET } from './offers.ts';
import { DEFAULT_THRESHOLDS, type Thresholds } from './scoring.ts';

const here = dirname(fileURLToPath(import.meta.url));

export interface Config {
  network: string;
  relayUrl: string;
  esploraUrl: string;
  dbPath: string;
  keyFile: string;
  secretHex: string | undefined;
  catalogPath: string;
  httpHost: string;
  httpPort: number;
  publish: boolean;
  scoreEverySecs: number;
  thresholds: Thresholds;
}

const num = (v: string | undefined, d: number) => {
  if (v === undefined || v === '') return d;
  const n = Number(v);
  if (!Number.isFinite(n)) throw new Error(`not a number: ${v}`);
  return n;
};

export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const network = env.MOSAIK_INDEXER_NETWORK ?? MUTINYNET;
  if (network !== MUTINYNET) throw new Error(`mosaik-indexer runs on Mutinynet (${MUTINYNET}) only, not ${network}`);
  const [httpHost, httpPort] = (env.MOSAIK_INDEXER_HTTP ?? '127.0.0.1:8787').split(':');
  return {
    network,
    relayUrl: env.MOSAIK_INDEXER_RELAY ?? 'wss://relay.kaleidoswap.com',
    esploraUrl: env.MOSAIK_INDEXER_ESPLORA ?? 'https://esplora.signet.kaleidoswap.com',
    dbPath: env.MOSAIK_INDEXER_DB ?? resolve(here, '..', 'mosaik-indexer.db'),
    keyFile: env.MOSAIK_INDEXER_KEY_FILE ?? join(homedir(), '.config', 'mosaik-indexer', 'mutinynet.key'),
    secretHex: env.MOSAIK_INDEXER_SECRET_HEX || undefined,
    catalogPath: env.MOSAIK_INDEXER_CATALOG ?? resolve(here, '..', '..', '..', 'webapp', 'public', '.well-known', 'rgb', 'tokens.json'),
    httpHost: httpHost || '127.0.0.1',
    httpPort: num(httpPort, 8787),
    publish: env.MOSAIK_INDEXER_PUBLISH === '1',
    scoreEverySecs: num(env.MOSAIK_INDEXER_SCORE_EVERY, 600),
    thresholds: {
      minSettled: num(env.MOSAIK_VERIFIED_MIN_SETTLED, DEFAULT_THRESHOLDS.minSettled),
      minCounterparties: num(env.MOSAIK_VERIFIED_MIN_COUNTERPARTIES, DEFAULT_THRESHOLDS.minCounterparties),
      minPresence: num(env.MOSAIK_VERIFIED_MIN_PRESENCE, DEFAULT_THRESHOLDS.minPresence),
      maxAbortRate: num(env.MOSAIK_VERIFIED_MAX_ABORT_RATE, DEFAULT_THRESHOLDS.maxAbortRate),
    },
  };
}
