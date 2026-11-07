import clsx from 'clsx';
import { useEffect, useRef, useState } from 'react';
import { shortId } from '../../lib/format';
import { fmtPrice } from '../../lib/units';
import { Icon } from '../ui';
import type { Market } from './book';

const short = (id: string) => shortId(id.replace('rgb:', ''), 6, 4);

function Stat({ label, value, tone, hint }: { label: string; value: string; tone?: string; hint?: string }) {
  return (
    <div className="flex min-w-0 flex-col" title={hint}>
      <dt className="text-tiny text-content-tertiary">{label}</dt>
      <dd className={clsx('truncate text-body font-medium tabular-nums', tone ?? 'text-content-primary')}>{value}</dd>
    </div>
  );
}

function PairSelect({ markets, selected, onSelect }: { markets: Market[]; selected?: Market; onSelect: (id: string) => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', down);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', down); document.removeEventListener('keydown', key); };
  }, [open]);

  return (
    <div className="relative" ref={box}>
      <button
        type="button"
        className="flex items-center gap-3 rounded-xl bg-surface-elevated px-3 py-2 text-left transition hover:bg-surface-high"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="Choose market"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-primary/15 text-tiny font-bold text-primary">
          {(selected?.ticker ?? '?').slice(0, 2).toUpperCase()}
        </span>
        <span className="min-w-0">
          <span className="block text-subhead font-bold leading-5">{selected?.ticker ?? 'asset'}<span className="text-content-tertiary"> / BTC</span></span>
          <span className="block font-mono text-tiny text-content-tertiary">{selected ? short(selected.assetId) : 'no market'}</span>
        </span>
        <Icon name="chevron" size={16} className={clsx('text-content-secondary transition', open && 'rotate-180')} />
      </button>
      {open && (
        <ul role="listbox" aria-label="Markets" className="absolute left-0 top-full z-30 mt-2 max-h-72 w-72 max-w-[85vw] overflow-auto rounded-2xl border border-border-subtle bg-surface-overlay p-1.5 shadow-popover">
          {markets.map((m) => (
            <li key={m.assetId} role="option" aria-selected={m.assetId === selected?.assetId}>
              <button
                type="button"
                className={clsx('flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left hover:bg-surface-elevated', m.assetId === selected?.assetId && 'bg-surface-elevated')}
                onClick={() => { onSelect(m.assetId); setOpen(false); }}
              >
                <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/15 text-tiny font-bold text-primary">{(m.ticker ?? '?').slice(0, 2).toUpperCase()}</span>
                <span className="min-w-0 flex-1">
                  <span className="block text-caption font-medium">{m.ticker ?? 'asset'} / BTC</span>
                  <span className="block truncate font-mono text-tiny text-content-tertiary" title={m.assetId}>{short(m.assetId)}{m.name ? ` · ${m.name}` : ''}</span>
                </span>
                <span className="text-tiny text-content-tertiary tabular-nums">{m.asks + m.bids} {m.asks + m.bids === 1 ? 'order' : 'orders'}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

interface Props {
  markets: Market[];
  selected?: Market;
  onSelect: (id: string) => void;
  networks: string[];
  network?: string;
  onNetwork: (n: string) => void;
  bestBid?: number;
  bestAsk?: number;
  lastFill?: number;
  orders: number;
}

/** Pair selector and the numbers a trader looks at first. */
export function PairBar({ markets, selected, onSelect, networks, network, onNetwork, bestBid, bestAsk, lastFill, orders }: Props) {
  const both = bestBid !== undefined && bestAsk !== undefined;
  const mid = both ? (bestBid + bestAsk) / 2 : undefined;
  const spread = both ? bestAsk - bestBid : undefined;
  const crossed = spread !== undefined && spread < 0;
  const t = selected?.ticker ?? 'unit';
  return (
    <header className="flex flex-wrap items-center gap-x-6 gap-y-3 rounded-2xl bg-surface-card/[0.04] px-4 py-3">
      <PairSelect markets={markets} selected={selected} onSelect={onSelect} />
      <dl className="grid w-full min-w-0 grid-cols-3 gap-x-4 gap-y-2 sm:grid-cols-5 lg:flex lg:w-auto lg:flex-1 lg:flex-wrap lg:gap-x-8">
        <Stat label="Last fill" value={lastFill !== undefined ? fmtPrice(lastFill) : '—'} hint="Your latest completed swap in this market from this browser. There is no public trade feed yet." />
        <Stat label="Best bid" value={bestBid !== undefined ? fmtPrice(bestBid) : '—'} tone="text-status-success" hint={`Highest price a maker pays per ${t} (sat)`} />
        <Stat label="Best ask" value={bestAsk !== undefined ? fmtPrice(bestAsk) : '—'} tone="text-status-danger" hint={`Lowest price a maker sells at per ${t} (sat)`} />
        <Stat
          label="Spread"
          value={spread !== undefined && mid ? `${crossed ? '−' : ''}${fmtPrice(Math.abs(spread))} · ${(Math.abs(spread) / mid * 100).toFixed(2)}%` : '—'}
          hint={crossed ? 'Bids are above asks: the orders have not been matched yet' : 'Best ask minus best bid'}
        />
        <Stat label="Open orders" value={String(orders)} />
      </dl>
      <div className="flex items-center gap-1.5 text-tiny text-content-tertiary">
        <span>sat per {t} ·</span>
        {networks.length > 1 ? (
          <div role="group" aria-label="Network" className="flex gap-1">
            {networks.map((n) => (
              <button key={n} type="button" aria-pressed={n === network} onClick={() => onNetwork(n)} className={clsx('rounded-full px-2 py-0.5 font-mono', n === network ? 'bg-primary/15 text-primary' : 'bg-surface-card/5 text-content-secondary hover:text-content-primary')}>{n}</button>
            ))}
          </div>
        ) : (
          <span className="rounded-full bg-surface-card/5 px-2 py-0.5 font-mono text-content-secondary">{network ?? '—'}</span>
        )}
      </div>
    </header>
  );
}
