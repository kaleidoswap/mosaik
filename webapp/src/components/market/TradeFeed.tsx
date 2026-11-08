import clsx from 'clsx';
import { fillUnitPrice, type RgbFill } from '../../api/rgbp2p';
import { shortId } from '../../lib/format';
import { fmtPrice, fmtUnits } from '../../lib/units';

const ago = (at: number, now: number) => {
  const s = Math.max(0, now - at);
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m` : s < 86_400 ? `${Math.floor(s / 3600)}h` : `${Math.floor(s / 86_400)}d`;
};

/** Trades makers say they completed in this market. Self-reported: a maker can publish what it likes. */
export function TradeFeed({ fills, ticker, nowSec }: { fills: RgbFill[]; ticker: string; nowSec: number }) {
  const shown = fills.slice(0, 12);
  return (
    <section aria-label="Recent trades" className="rounded-2xl bg-surface-card/[0.04] p-4">
      <header className="mb-2 flex items-baseline justify-between">
        <h2 className="text-subhead font-medium">Recent trades</h2>
        <span className="text-tiny text-content-tertiary" title="The relay carries what makers publish. A fill is not proof that a swap settled.">reported by makers</span>
      </header>
      {shown.length === 0 ? (
        <p className="py-3 text-center text-caption text-content-tertiary">No trades reported yet.</p>
      ) : (
        <ul className="flex flex-col text-caption tabular-nums">
          {shown.map((f) => {
            const p = fillUnitPrice(f);
            return (
              <li key={f.key + f.at} className="grid grid-cols-[1fr_1fr_auto] items-center gap-2 py-1">
                <span className={clsx('font-medium', f.side === 'sell' ? 'text-status-success' : 'text-status-danger')} title={f.side === 'sell' ? 'The maker sold' : 'The maker bought'}>{p !== null ? fmtPrice(p) : '–'}</span>
                <span className="text-right">{fmtUnits(f.amount, f.precision)} <span className="text-content-tertiary">{ticker}</span></span>
                <span className="text-right text-tiny text-content-tertiary" title={`${new Date(f.at * 1000).toLocaleString()} · maker ${shortId(f.maker, 4, 4)}`}>{ago(f.at, nowSec)}</span>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
