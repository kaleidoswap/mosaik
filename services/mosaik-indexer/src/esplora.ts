/** The few Esplora calls the indexer makes. Note: Esplora's scripthash is sha256(script), not reversed. */

import { createHash } from 'node:crypto';
import type { EsploraTx } from './settlement.ts';

export interface Chain {
  addressTxs(address: string): Promise<EsploraTx[]>;
  scriptTxs(scriptHex: string): Promise<EsploraTx[]>;
  tx(txid: string): Promise<EsploraTx | null>;
}

export class Esplora implements Chain {
  readonly base: string;
  constructor(base: string) {
    this.base = base.replace(/\/+$/, '');
  }

  private async get<T>(path: string): Promise<T | null> {
    const r = await fetch(this.base + path, { signal: AbortSignal.timeout(20_000), headers: { accept: 'application/json' } });
    if (r.status === 404 || r.status === 400) return null;
    if (!r.ok) throw new Error(`esplora ${path}: HTTP ${r.status}`);
    return (await r.json()) as T;
  }

  async addressTxs(address: string) {
    return (await this.get<EsploraTx[]>(`/address/${encodeURIComponent(address)}/txs`)) ?? [];
  }

  async scriptTxs(scriptHex: string) {
    const hash = createHash('sha256').update(Buffer.from(scriptHex, 'hex')).digest('hex');
    return (await this.get<EsploraTx[]>(`/scripthash/${hash}/txs`)) ?? [];
  }

  async tx(txid: string) {
    return this.get<EsploraTx>(`/tx/${txid}`);
  }
}
