/** Numbers a market header shows, from the fills makers have reported. */
import { fillUnitPrice, type RgbFill } from '../../api/rgbp2p';

export const DAY = 86_400;

export interface Point {
  at: number;
  price: number;
  units: number;
}

export interface MarketStats {
  last?: number;
  /** Percent against the first fill of the last 24 hours, when there are two. */
  change24h?: number;
  /** BTC leg of the last 24 hours, in sats. */
  volume24hSat: number;
  /** Asset units traded in the last 24 hours. */
  units24h: number;
  trades24h: number;
  /** Oldest first, for the chart. */
  points: Point[];
}

export function marketFills(fills: RgbFill[], assetId: string | undefined, network: string | undefined): RgbFill[] {
  return fills.filter((f) => f.assetId === assetId && f.network === network);
}

export function marketStats(fills: RgbFill[], now: number): MarketStats {
  const points: Point[] = [];
  for (const f of fills) {
    const price = fillUnitPrice(f);
    if (price !== null) points.push({ at: f.at, price, units: f.amount / 10 ** f.precision });
  }
  points.sort((a, b) => a.at - b.at);
  const day = fills.filter((f) => f.at >= now - DAY);
  const dayPoints = points.filter((p) => p.at >= now - DAY);
  const first = dayPoints[0];
  const last = points.at(-1);
  return {
    last: last?.price,
    change24h: first && last && dayPoints.length > 1 && first.price > 0 ? ((last.price - first.price) / first.price) * 100 : undefined,
    volume24hSat: day.reduce((s, f) => s + f.priceSat, 0),
    units24h: dayPoints.reduce((s, p) => s + p.units, 0),
    trades24h: day.length,
    points,
  };
}
