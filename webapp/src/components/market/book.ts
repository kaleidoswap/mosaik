/** The order book as a DEX shows it: one market per asset, asks and bids sorted, depth running outward. */
import type { CatalogAsset } from '../../api/catalog';
import { unitPriceSat, type RgbOffer } from '../../api/rgbp2p';

export interface Market {
  assetId: string;
  ticker?: string;
  name?: string;
  precision: number;
  asks: number;
  bids: number;
}

export interface Level {
  offer: RgbOffer;
  /** sats per whole asset unit */
  price: number;
  /** whole asset units */
  units: number;
  /** running total of units from the best price outward */
  cum: number;
}

const unitsOf = (o: RgbOffer) => o.amount / 10 ** o.precision;

/** The assets with orders on a network, busiest first. */
export function marketsOf(offers: RgbOffer[], catalog: CatalogAsset[] = []): Market[] {
  const m = new Map<string, Market>();
  for (const a of catalog) m.set(a.contractId, { assetId: a.contractId, ticker: a.ticker, name: a.name, precision: a.precision, asks: 0, bids: 0 });
  for (const o of offers) {
    const cur = m.get(o.assetId) ?? { assetId: o.assetId, ticker: o.ticker, name: o.name, precision: o.precision, asks: 0, bids: 0 };
    cur.ticker ??= o.ticker;
    cur.name ??= o.name;
    if (o.side === 'sell') cur.asks += 1;
    else cur.bids += 1;
    m.set(o.assetId, cur);
  }
  return [...m.values()].sort((a, b) => b.asks + b.bids - (a.asks + a.bids) || a.assetId.localeCompare(b.assetId));
}

const level = (offer: RgbOffer, cum: number): Level => ({ offer, price: unitPriceSat(offer) ?? 0, units: unitsOf(offer), cum });

/** Asks best (lowest) first, bids best (highest) first, each with its running depth. */
export function ladder(offers: RgbOffer[]): { asks: Level[]; bids: Level[] } {
  const priced = offers.filter((o) => unitPriceSat(o) !== null);
  const by = (dir: 1 | -1) => (a: RgbOffer, b: RgbOffer) =>
    dir * ((unitPriceSat(a) ?? 0) - (unitPriceSat(b) ?? 0)) || a.createdAt - b.createdAt;
  const run = (list: RgbOffer[]) => {
    let cum = 0;
    return list.map((o) => level(o, (cum += unitsOf(o))));
  };
  return {
    asks: run(priced.filter((o) => o.side === 'sell').sort(by(1))),
    bids: run(priced.filter((o) => o.side === 'buy').sort(by(-1))),
  };
}
