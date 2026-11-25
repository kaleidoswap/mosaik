/** The curated catalog (`mosaik-catalog/v1`), as `webapp/public/.well-known/rgb/tokens.json` holds it. */

export const CATALOG_FORMAT = 'mosaik-catalog/v1';

export interface CatalogToken {
  contract_id: string;
  ticker: string;
  name: string;
  precision: number;
  [extra: string]: unknown;
}

export interface Catalog {
  format: typeof CATALOG_FORMAT;
  version: string;
  network: string;
  tokens: CatalogToken[];
  [extra: string]: unknown;
}

const ASSET_RE = /^rgb:[A-Za-z0-9~_:-]{8,120}$/;
const PUBKEY_RE = /^[0-9a-f]{64}$/;

/** The catalog, checked; throws with the first problem found. */
export function parseCatalog(raw: unknown): Catalog {
  const c = raw as Partial<Catalog> | null;
  if (!c || typeof c !== 'object') throw new Error('catalog: not an object');
  if (c.format !== CATALOG_FORMAT) throw new Error(`catalog: format must be ${CATALOG_FORMAT}`);
  if (typeof c.version !== 'string' || !c.version) throw new Error('catalog: version is required');
  if (typeof c.network !== 'string' || !c.network) throw new Error('catalog: network is required');
  if (!Array.isArray(c.tokens)) throw new Error('catalog: tokens must be an array');
  const seen = new Set<string>();
  const tickers = new Set<string>();
  for (const t of c.tokens as Partial<CatalogToken>[]) {
    if (typeof t.contract_id !== 'string' || !ASSET_RE.test(t.contract_id)) throw new Error(`catalog: bad contract_id ${String(t.contract_id)}`);
    if (seen.has(t.contract_id)) throw new Error(`catalog: ${t.contract_id} listed twice`);
    seen.add(t.contract_id);
    if (typeof t.ticker !== 'string' || !/^[A-Za-z0-9.\-]{1,12}$/.test(t.ticker)) throw new Error(`catalog: bad ticker for ${t.contract_id}`);
    // one ticker, one asset id: the curated tier never merges two assets under a name
    const key = t.ticker.toUpperCase();
    if (tickers.has(key)) throw new Error(`catalog: ticker ${t.ticker} used twice`);
    tickers.add(key);
    if (typeof t.name !== 'string' || !t.name || t.name.length > 80) throw new Error(`catalog: bad name for ${t.contract_id}`);
    if (typeof t.precision !== 'number' || !Number.isInteger(t.precision) || t.precision < 0 || t.precision > 18)
      throw new Error(`catalog: bad precision for ${t.contract_id}`);
    const im = t.issuer_makers;
    if (im !== undefined && (!Array.isArray(im) || !im.every((p) => typeof p === 'string' && PUBKEY_RE.test(p))))
      throw new Error(`catalog: issuer_makers for ${t.contract_id} must be hex pubkeys`);
  }
  return c as Catalog;
}
