import { fmtPrice } from '../../lib/units';
import type { Point } from './stats';

const W = 640;
const H = 160;
const PAD = { l: 8, r: 56, t: 10, b: 18 };

/** Fills as a price line over time. Few fills are normal here, so each one is also a dot. */
export function PriceChart({ points, ticker }: { points: Point[]; ticker: string }) {
  if (points.length === 0) {
    return (
      <section aria-label="Price chart" className="rounded-2xl bg-surface-card/[0.04] p-4">
        <h2 className="text-subhead font-medium">Price</h2>
        <p className="mt-6 pb-6 text-center text-caption text-content-tertiary">No trades reported for {ticker} yet. The line appears with the first one.</p>
      </section>
    );
  }
  const first = points[0]!;
  const last = points[points.length - 1]!;
  const t0 = first.at;
  const t1 = Math.max(last.at, t0 + 1);
  const lo = Math.min(...points.map((p) => p.price));
  const hi = Math.max(...points.map((p) => p.price));
  const span = hi - lo || Math.max(hi * 0.1, 1);
  const x = (t: number) => PAD.l + ((t - t0) / (t1 - t0 || 1)) * (W - PAD.l - PAD.r);
  const y = (p: number) => PAD.t + (1 - (p - (lo - span * 0.1)) / (span * 1.2)) * (H - PAD.t - PAD.b);
  const path = points.map((p, i) => `${i ? 'L' : 'M'}${x(p.at).toFixed(1)},${y(p.price).toFixed(1)}`).join(' ');
  const up = points.length < 2 || last.price >= first.price;
  const tone = up ? 'var(--chart-up, #2fbf71)' : 'var(--chart-down, #e5534b)';
  const day = (t: number) => new Date(t * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return (
    <section aria-label="Price chart" className="rounded-2xl bg-surface-card/[0.04] p-4">
      <header className="mb-1 flex items-baseline justify-between">
        <h2 className="text-subhead font-medium">Price</h2>
        <span className="text-tiny text-content-tertiary">sat per {ticker} · {points.length} {points.length === 1 ? 'trade' : 'trades'}</span>
      </header>
      <svg viewBox={`0 0 ${W} ${H}`} className="h-40 w-full" role="img" aria-label={`Price of ${ticker}, from ${fmtPrice(first.price)} to ${fmtPrice(last.price)} sat`}>
        {[hi, (hi + lo) / 2, lo].map((v, i) => (
          <g key={i}>
            <line x1={PAD.l} x2={W - PAD.r} y1={y(v)} y2={y(v)} stroke="currentColor" strokeOpacity="0.1" strokeDasharray="3 4" />
            <text x={W - PAD.r + 6} y={y(v) + 4} fontSize="11" fill="currentColor" fillOpacity="0.55">{fmtPrice(v)}</text>
          </g>
        ))}
        <path d={path} fill="none" stroke={tone} strokeWidth="2" strokeLinejoin="round" />
        {points.map((p, i) => <circle key={i} cx={x(p.at)} cy={y(p.price)} r="3" fill={tone}><title>{`${fmtPrice(p.price)} sat · ${new Date(p.at * 1000).toLocaleString()}`}</title></circle>)}
        <text x={PAD.l} y={H - 2} fontSize="11" fill="currentColor" fillOpacity="0.55">{day(t0)}</text>
        <text x={W - PAD.r} y={H - 2} fontSize="11" textAnchor="end" fill="currentColor" fillOpacity="0.55">{day(t1)}</text>
      </svg>
    </section>
  );
}
