import clsx from 'clsx';
import { useState } from 'react';
import type { RgbOffer } from '../../api/rgbp2p';
import { shortId } from '../../lib/format';
import { fmtLeft, fmtPrice, fmtUnits } from '../../lib/units';
import type { Level } from './book';
import type { MakerMark } from './listing';
import { MakerMarks } from './Marks';

const ROWS = 4;
const COLS = 'grid-cols-[1.1fr_1fr_1fr] md:grid-cols-[1.1fr_1fr_1fr_1fr_0.6fr]';

function Row({ l, kind, max, ticker, nowSec, onPick, markFor }: { l: Level; kind: 'ask' | 'bid'; max: number; ticker: string; nowSec: number; onPick: (o: RgbOffer) => void; markFor?: (maker: string) => MakerMark }) {
  const o = l.offer;
  const mark = markFor?.(o.maker);
  return (
    <li>
      <button
        type="button"
        onClick={() => onPick(o)}
        className={clsx('group relative grid w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-caption tabular-nums transition-colors hover:bg-surface-elevated focus-visible:bg-surface-elevated', COLS)}
        title={`${kind === 'ask' ? 'Buy' : 'Sell'} ${fmtUnits(o.amount, o.precision)} ${ticker} for ${o.priceSat.toLocaleString()} sat. The whole order is taken at once.`}
        aria-label={`${kind === 'ask' ? 'Buy' : 'Sell'} ${fmtUnits(o.amount, o.precision)} ${ticker} at ${fmtPrice(l.price)} sat each, ${o.priceSat} sat total. Opens the review.`}
      >
        <span aria-hidden className={clsx('pointer-events-none absolute inset-y-0 right-0 rounded-lg', kind === 'ask' ? 'bg-status-danger-subtle' : 'bg-status-success-subtle')} style={{ width: `${Math.max(4, (l.cum / max) * 100)}%` }} />
        <span className={clsx('relative font-medium', kind === 'ask' ? 'text-status-danger' : 'text-status-success')}>{fmtPrice(l.price)}</span>
        <span className="relative text-right text-content-primary">{fmtUnits(o.amount, o.precision)}</span>
        <span className="relative text-right text-content-secondary">{o.priceSat.toLocaleString()}</span>
        <span className="relative hidden items-center justify-end gap-1 text-right font-mono text-tiny text-content-tertiary md:flex" title={o.maker}>{mark && <MakerMarks mark={mark} compact />}{shortId(o.maker, 4, 4)}</span>
        <span className="relative hidden text-right text-tiny text-content-tertiary md:block">{fmtLeft(o.expiry, nowSec)}</span>
      </button>
    </li>
  );
}

interface Props {
  asks: Level[];
  bids: Level[];
  ticker: string;
  nowSec: number;
  onPick: (o: RgbOffer) => void;
  /** Verified-maker badge and issuer label for an order's maker. */
  markFor?: (maker: string) => MakerMark;
}

/**
 * Depth ladder: sellers above, buyers below, the best price of each side next to the middle. Every row is
 * one maker's order and is taken whole; clicking one opens the review for it.
 */
export function Ladder({ asks, bids, ticker, nowSec, onPick, markFor }: Props) {
  const [all, setAll] = useState(false);
  const shownAsks = all ? asks : asks.slice(0, ROWS);
  const shownBids = all ? bids : bids.slice(0, ROWS);
  const max = Math.max(1e-9, shownAsks.at(-1)?.cum ?? 0, shownBids.at(-1)?.cum ?? 0);
  const bestAsk = asks[0]?.price;
  const bestBid = bids[0]?.price;
  const mid = bestAsk !== undefined && bestBid !== undefined ? (bestAsk + bestBid) / 2 : bestAsk ?? bestBid;
  const spreadPct = bestAsk !== undefined && bestBid !== undefined && mid ? ((bestAsk - bestBid) / mid) * 100 : undefined;
  const hidden = asks.length + bids.length - shownAsks.length - shownBids.length;

  return (
    <section aria-label="Order book" className="flex min-w-0 flex-col rounded-2xl bg-surface-card/[0.04] p-3">
      <header className="flex items-baseline justify-between px-2 pb-2">
        <h2 className="text-subhead font-medium">Order book</h2>
        <span className="text-caption text-content-tertiary">{asks.length} selling · {bids.length} buying</span>
      </header>
      <div className={clsx('grid gap-2 px-2 pb-1 text-tiny text-content-tertiary', COLS)}>
        <span>Price (sat)</span>
        <span className="text-right">Amount ({ticker})</span>
        <span className="text-right">Total (sat)</span>
        <span className="hidden text-right md:block">Maker</span>
        <span className="hidden text-right md:block">Left</span>
      </div>

      <ul className="flex flex-col justify-end">
        {shownAsks.length ? [...shownAsks].reverse().map((l) => <Row key={l.offer.key} l={l} kind="ask" max={max} ticker={ticker} nowSec={nowSec} onPick={onPick} markFor={markFor} />) : <li className="px-2 py-3 text-caption text-content-tertiary">No sellers right now</li>}
      </ul>

      <div className="my-1.5 flex items-baseline justify-between rounded-xl bg-surface-inset/25 px-3 py-2">
        <strong className="text-subhead tabular-nums">{mid !== undefined ? fmtPrice(mid) : '–'}</strong>
        <span className="text-caption tabular-nums text-content-tertiary">
          {spreadPct !== undefined ? `spread ${Math.abs(spreadPct).toFixed(2)}%` : `sat per ${ticker}`}
        </span>
      </div>

      <ul className="flex flex-col">
        {shownBids.length ? shownBids.map((l) => <Row key={l.offer.key} l={l} kind="bid" max={max} ticker={ticker} nowSec={nowSec} onPick={onPick} markFor={markFor} />) : <li className="px-2 py-3 text-caption text-content-tertiary">No buyers right now</li>}
      </ul>

      {(hidden > 0 || all) && (asks.length > ROWS || bids.length > ROWS) && (
        <button type="button" className="mt-2 self-center rounded-lg px-3 py-1 text-caption text-content-secondary hover:bg-surface-elevated hover:text-content-primary" onClick={() => setAll((v) => !v)}>
          {all ? 'Show fewer' : `Show ${hidden} more`}
        </button>
      )}
      <p className="mt-3 px-2 text-tiny text-content-tertiary">Each row is one maker’s order, filled whole. Tap a row to review and take it.</p>
    </section>
  );
}
