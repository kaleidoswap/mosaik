import { useQuery } from '@tanstack/react-query';
import { fetchSignedList, LIST_PUBKEY } from './signed';

export interface CatalogAsset {
  contractId: string;
  ticker: string;
  name: string;
  precision: number;
  issuedSupply: string;
  description: string;
  demo: true;
  /** Maker keys the list says are run by, or for, the issuer. */
  issuerMakers: string[];
}
export interface Catalog {
  version: string;
  networkId: 'bitcoin:mutinynet';
  assets: CatalogAsset[];
  /**
   * Set when this list came from the kind-30055 event and its signature checked out against the
   * Kaleido list key. Only then is an asset in it "Kaleido curated".
   */
  signed?: { pubkey: string; eventId: string; signedAt: number };
  /** Why the signed list was not used, when it was not (the well-known file is shown instead). */
  signedError?: string;
}

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
    const im = a.issuer_makers;
    if (im !== undefined && (!Array.isArray(im) || !im.every((k) => typeof k === 'string' && /^[0-9a-f]{64}$/.test(k)))) throw new Error('Invalid catalog asset.');
    seen.add(a.contract_id);
    return { contractId: a.contract_id, ticker: a.ticker, name: a.name, precision: Number(a.precision), issuedSupply: a.issued_supply, description: a.description, demo: true, issuerMakers: (im as string[] | undefined) ?? [] };
  });
  return { version: String(b.version ?? ''), networkId: 'bitcoin:mutinynet', assets };
}

/**
 * The signed list from the relay first; the well-known file when that cannot be had. The file is
 * still good metadata, but nothing from it is shown as Kaleido curated.
 */
export async function loadCatalog(network: string, signal?: AbortSignal): Promise<Catalog> {
  let signedError: string;
  try {
    const r = await fetchSignedList(network);
    if (r.ok) return { ...parseCatalog(r.value.content, network), signed: { pubkey: LIST_PUBKEY, eventId: r.value.eventId, signedAt: r.value.signedAt } };
    signedError = r.error;
  } catch (e) {
    signedError = e instanceof Error ? e.message : 'relay unavailable';
  }
  const r = await fetch('/.well-known/rgb/tokens.json', { signal, cache: 'no-cache' });
  if (!r.ok) throw new Error('Catalog unavailable');
  return { ...parseCatalog(await r.json(), network), signedError };
}

export function useCatalog(network: string) {
  return useQuery({ queryKey: ['mosaik-catalog', network, LIST_PUBKEY], staleTime: 60_000, retry: 1,
    queryFn: ({ signal }) => loadCatalog(network, signal),
  });
}
