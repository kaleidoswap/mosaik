import { useQuery } from '@tanstack/react-query';

export interface CatalogAsset {
  contractId: string;
  ticker: string;
  name: string;
  precision: number;
  issuedSupply: string;
  description: string;
  demo: true;
}
export interface Catalog { version: string; networkId: 'bitcoin:mutinynet'; assets: CatalogAsset[] }

/** A curated list is a source of metadata, never proof of issuer identity or backing. */
export function parseCatalog(body: unknown, network: string): Catalog {
  const b = body as Record<string, unknown> | null;
  if (network !== 'signetcustom' || b?.network !== network || b.network_id !== 'bitcoin:mutinynet' || !Array.isArray(b.tokens)) {
    throw new Error('The catalog belongs to a different network or has an invalid format.');
  }
  const seen = new Set<string>();
  const assets = b.tokens.map((raw: unknown): CatalogAsset => {
    const a = raw as Record<string, unknown>;
    if (!a || typeof a.contract_id !== 'string' || !/^rgb:[A-Za-z0-9~_:-]{8,120}$/.test(a.contract_id) || seen.has(a.contract_id)
      || typeof a.ticker !== 'string' || !/^[A-Z0-9]{1,8}$/.test(a.ticker)
      || typeof a.name !== 'string' || a.name.length > 64 || !Number.isInteger(a.precision) || Number(a.precision) < 0 || Number(a.precision) > 8
      || typeof a.issued_supply !== 'string' || !/^\d+$/.test(a.issued_supply) || a.demo !== true
      || typeof a.description !== 'string' || a.description.length > 500) throw new Error('Invalid catalog asset.');
    seen.add(a.contract_id);
    return { contractId: a.contract_id, ticker: a.ticker, name: a.name, precision: Number(a.precision), issuedSupply: a.issued_supply, description: a.description, demo: true };
  });
  return { version: String(b.version ?? ''), networkId: 'bitcoin:mutinynet', assets };
}
export function useCatalog(network: string) {
  return useQuery({ queryKey: ['mosaik-catalog', network], staleTime: 60_000, retry: 1,
    queryFn: async ({ signal }) => {
      const r = await fetch('/.well-known/rgb/tokens.json', { signal, cache: 'no-cache' });
      if (!r.ok) throw new Error('Catalog unavailable');
      return parseCatalog(await r.json(), network);
    },
  });
}
