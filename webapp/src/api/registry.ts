/**
 * What the RGBMap index lists, to put next to what a maker says an asset is. The index states
 * facts (listed, same ticker as others); none of it says an asset is safe, so nothing here is a
 * verdict. The whole list is read once per network: asking about one asset would tell the index
 * what the visitor is looking at.
 */

import { useQuery } from '@tanstack/react-query';
import { safeLabel } from '../lib/text';

export const REGISTRY_URL = import.meta.env.VITE_RGB_REGISTRY_URL ?? 'https://api.rgbmap.org';

export interface Listed {
  contractId: string;
  ticker: string;
  name: string;
  listedBy: string[];
}

export function parseRegistryList(body: unknown): Listed[] {
  const assets = (body as { assets?: unknown })?.assets;
  if (!Array.isArray(assets)) return [];
  const out: Listed[] = [];
  for (const a of assets as Array<Record<string, unknown>>) {
    const contractId = typeof a?.contract_id === 'string' ? a.contract_id : '';
    if (!/^rgb:[A-Za-z0-9~_:-]{8,120}$/.test(contractId)) continue;
    const by = (a.status as { listed_by?: unknown } | undefined)?.listed_by;
    out.push({
      contractId,
      ticker: safeLabel(a.ticker, 12) ?? '',
      name: safeLabel(a.name, 40) ?? '',
      listedBy: Array.isArray(by) ? by.map((x) => safeLabel(x, 40)).filter((x): x is string => !!x) : [],
    });
  }
  return out;
}

/** rgb-lib network names (`signetcustom`) to the index's (`signet`). */
export const registryNetwork = (n: string): string => (n.startsWith('signet') ? 'signet' : n.startsWith('testnet') ? 'testnet' : n);

export function useRegistry(network: string | undefined) {
  return useQuery({
    queryKey: ['rgb-registry', network],
    enabled: !!network,
    staleTime: 5 * 60_000,
    retry: 1,
    queryFn: async ({ signal }) => {
      const r = await fetch(`${REGISTRY_URL}/v1/assets?network=${encodeURIComponent(registryNetwork(network!))}`, { signal });
      if (!r.ok) throw new Error(`registry ${r.status}`);
      return parseRegistryList(await r.json());
    },
  });
}
