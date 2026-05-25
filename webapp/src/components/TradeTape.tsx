import type { Trade } from '../api/types';
import { fmtSats, fmtRate, shortId } from '../lib/format';

interface Props {
  trades: Trade[];
}

export function TradeTape({ trades }: Props) {
  return (
    <section className="panel flex flex-col min-h-0">
      <header className="panel-header">
        <h2 className="panel-title">Trade tape</h2>
        <span className="text-[10px] font-mono text-zinc-500">{trades.length} settled</span>
      </header>
      {trades.length === 0 ? (
        <div className="p-6 text-center text-sm text-zinc-500">No trades this session.</div>
      ) : (
        <ul className="divide-y divide-ink-500/40 overflow-auto font-mono text-sm">
          {trades.map((t, i) => (
            <li key={`${t.txid}-${i}`} className="flex items-center gap-3 px-4 py-2">
              <span className="text-zinc-400">#{trades.length - i}</span>
              <span className="text-zinc-200">{t.pair}</span>
              <span className="flex-1 text-zinc-500">
                {fmtSats(t.amount_a)} → {fmtSats(t.amount_b)}
              </span>
              <span className="text-mosaic-amber">{fmtRate(t.amount_a, t.amount_b)}</span>
              <span className="pill" title={`block ${t.block}`}>
                blk {t.block}
              </span>
              <span className="text-zinc-500" title={t.txid}>
                {shortId(t.txid, 8, 6)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
