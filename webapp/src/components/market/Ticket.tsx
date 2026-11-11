import clsx from 'clsx';
import { Button } from 'kaleido-ui';
import { useState } from 'react';
import type { RgbOffer } from '../../api/rgbp2p';
import { useWebRgb } from '../../api/webrgb';
import { shortId } from '../../lib/format';
import { fmtLeft, fmtPrice, fmtUnits } from '../../lib/units';
import { Icon, Pill } from '../ui';
import type { Level } from './book';
import { shortfalls } from './affordability';

interface Props {
  ticker: string;
  precision: number;
  asks: Level[];
  bids: Level[];
  nowSec: number;
  onTake: (o: RgbOffer) => void;
}

/**
 * Trade ticket, taker side only: Mosaik can fill an order a maker already posted, it cannot post one.
 * Buy fills the cheapest seller, Sell fills the highest buyer; the review dialog has the full terms.
 */
export function Ticket({ ticker, precision, asks, bids, nowSec, onTake }: Props) {
  const w = useWebRgb();
  const [side, setSide] = useState<'buy' | 'sell'>('buy');
  const best = (side === 'buy' ? asks : bids)[0];
  const o = best?.offer;
  const held = w.assets.find((a) => a.id === o?.assetId)?.balance;
  const connected = w.status === 'connected';
  const short = o && connected ? shortfalls(o, { sat: w.balance?.spendable, asset: held ?? 0, freeColorableUtxos: w.balance?.freeColorableUtxos ?? undefined }) : [];

  return (
    <section aria-label="Trade ticket" className="flex flex-col gap-3 rounded-2xl bg-surface-card/[0.04] p-4">
      <div role="tablist" aria-label="Direction" className="grid grid-cols-2 gap-1 rounded-xl bg-surface-inset/25 p-1">
        {(['buy', 'sell'] as const).map((s) => (
          <button
            key={s}
            role="tab"
            type="button"
            aria-selected={side === s}
            onClick={() => setSide(s)}
            className={clsx('rounded-lg py-2 text-caption font-bold transition', side === s ? (s === 'buy' ? 'bg-status-success text-primary-foreground' : 'bg-status-danger text-primary-foreground') : 'text-content-secondary hover:text-content-primary')}
          >
            {s === 'buy' ? 'Buy' : 'Sell'} {ticker}
          </button>
        ))}
      </div>

      {o && best ? (
        <div className="rounded-xl border border-border-subtle p-3">
          <div className="flex items-baseline justify-between text-tiny text-content-tertiary">
            <span>Best {side === 'buy' ? 'seller' : 'buyer'}</span>
            <span title={o.maker} className="font-mono">{shortId(o.maker, 5, 4)}</span>
          </div>
          <div className="mt-1 text-title font-bold tabular-nums">{fmtUnits(o.amount, precision)} <span className="text-body text-content-secondary">{ticker}</span></div>
          <dl className="mt-2 grid grid-cols-2 gap-y-1 text-caption">
            <dt className="text-content-secondary">Price</dt><dd className="text-right tabular-nums">{fmtPrice(best.price)} sat</dd>
            <dt className="text-content-secondary">{side === 'buy' ? 'You pay' : 'You receive'}</dt><dd className="text-right tabular-nums">{o.priceSat.toLocaleString()} sat</dd>
            <dt className="text-content-secondary">Fees</dt>
            <dd className="text-right tabular-nums">{o.platformFeeSat !== undefined && o.networkFeeSat !== undefined ? `${(o.platformFeeSat + o.networkFeeSat).toLocaleString()} sat` : 'in review'}</dd>
            <dt className="text-content-secondary">Expires</dt><dd className="text-right tabular-nums">{fmtLeft(o.expiry, nowSec)}</dd>
          </dl>
          {short.length > 0 && (
            <ul role="alert" className="mt-3 list-disc rounded-lg bg-status-warning-subtle py-2 pl-6 pr-3 text-tiny text-status-warning">
              {short.map((x) => (
                <li key={x.kind}>
                  {x.kind === 'sat' ? `You have ${x.have.toLocaleString()} sat; this needs ${x.need.toLocaleString()} with fees.`
                    : x.kind === 'asset' ? `You have ${fmtUnits(x.have, precision)} ${ticker}; this needs ${fmtUnits(x.need, precision)}.`
                    : 'Your wallet has no free coin for RGB. Create one in the extension first.'}
                </li>
              ))}
            </ul>
          )}
          <Button type="button" variant="cta" className="mt-3 w-full" onClick={() => onTake(o)}>
            Review and {side === 'buy' ? 'buy' : 'sell'}
          </Button>
          <p className="mt-2 text-center text-tiny text-content-secondary">Whole offer · Approval in your wallet</p>
        </div>
      ) : (
        <div className="rounded-xl border border-dashed border-border-default p-4 text-center text-caption text-content-tertiary">
          No {side === 'buy' ? 'sellers' : 'buyers'} for {ticker} right now.
        </div>
      )}

      <div className="flex items-start gap-2 rounded-xl bg-surface-card/5 px-3 py-2 text-tiny text-content-secondary">
        <Icon name="info" size={14} className="mt-0.5" />
        <p>Mosaik takes orders that makers have already posted. To post your own order, use the KaleidoSwap desktop app.</p>
      </div>

      <div className="border-t border-border-subtle pt-3">
        <div className="mb-1.5 flex items-center justify-between text-tiny text-content-tertiary">
          <span>Your wallet</span>
          {connected ? <Pill tone="success">connected</Pill> : <Pill>not connected</Pill>}
        </div>
        {connected ? (
          <dl className="grid grid-cols-2 gap-y-1 text-caption">
            <dt className="text-content-secondary">BTC for swaps</dt><dd className="text-right tabular-nums">{w.balance ? `${w.balance.spendable.toLocaleString()} sat` : '—'}</dd>
            <dt className="text-content-secondary">{ticker}</dt><dd className="text-right tabular-nums">{held !== undefined ? fmtUnits(held, precision) : '0'}</dd>
            <dt className="text-content-secondary">Free RGB coins</dt><dd className={clsx('text-right tabular-nums', w.balance?.freeColorableUtxos === 0 && 'text-status-warning')}>{w.balance?.freeColorableUtxos ?? '—'}</dd>
          </dl>
        ) : (
          <p className="text-caption text-content-tertiary">Connect your KaleidoSwap wallet to see balances here. You will be asked when you review an order.</p>
        )}
      </div>
    </section>
  );
}
