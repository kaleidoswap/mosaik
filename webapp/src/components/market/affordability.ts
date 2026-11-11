/** Whether the connected wallet can fund an order as it stands, before the review opens. */
import type { OrderSnapshot } from '../../api/swaps';

export interface Funds {
  /** Spendable sats, when known. */
  sat?: number;
  /** Units of the order's asset, in the smallest unit, when known. */
  asset?: number;
  freeColorableUtxos?: number;
}

export type Shortfall = { kind: 'sat'; need: number; have: number } | { kind: 'asset'; need: number; have: number } | { kind: 'utxo' };

export function shortfalls(o: Pick<OrderSnapshot, 'side' | 'amount' | 'priceSat' | 'platformFeeSat' | 'networkFeeSat' | 'rgbOutputSat'>, f: Funds): Shortfall[] {
  const out: Shortfall[] = [];
  const fees = (o.platformFeeSat ?? 0) + (o.networkFeeSat ?? 0);
  // a maker's sell order is bought by the taker, who pays the price and the fees; a buy order is sold into
  const needSat = o.side === 'sell' ? o.priceSat + fees + (o.rgbOutputSat ?? 0) : fees;
  if (f.sat !== undefined && f.sat < needSat) out.push({ kind: 'sat', need: needSat, have: f.sat });
  if (o.side === 'buy' && f.asset !== undefined && f.asset < o.amount) out.push({ kind: 'asset', need: o.amount, have: f.asset });
  if (f.freeColorableUtxos === 0) out.push({ kind: 'utxo' });
  return out;
}
