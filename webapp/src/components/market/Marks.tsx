import clsx from 'clsx';
import { shortId } from '../../lib/format';
import type { Listing, MakerMark, MarketMakerRow, Thresholds } from './listing';
import { VERIFIED_THRESHOLDS } from './listing';

const TIER_STYLE: Record<Listing['tier'], string> = {
  curated: 'bg-primary/15 text-primary',
  registry: 'bg-status-info-subtle text-status-info',
  unlisted: 'bg-status-warning-subtle text-status-warning',
  unknown: 'bg-surface-card/10 text-content-secondary',
};

/** The listing tier of a market, with what it means on hover. */
export function ListingChip({ listing, className }: { listing: Listing; className?: string }) {
  return (
    <span data-tier={listing.tier} title={listing.detail} className={clsx('inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-tiny font-medium', TIER_STYLE[listing.tier], className)}>
      {listing.tier === 'unlisted' && <span aria-hidden className="mr-1">!</span>}
      {listing.label}
    </span>
  );
}

const pct = (x: number | null | undefined) => (x === null || x === undefined ? '—' : `${(x * 100).toFixed(1)}%`);

function scoreHint(m: MakerMark, t: Thresholds) {
  const s = m.score;
  if (!s) return 'No signed score for this maker yet.';
  return [
    `${s.settled} settled trades with ${s.counterparties} distinct counterparties, ${s.volumeSat.toLocaleString()} sat (30 days, verified on-chain)`,
    `presence ${pct(s.presence)} · aborts ${pct(s.abortRate)}`,
    m.verified ? 'Verified: meets every threshold.' : `Verified needs ≥${t.minSettled} trades, ≥${t.minCounterparties} counterparties, presence ≥${t.minPresence * 100}%, aborts ≤${t.maxAbortRate * 100}%.`,
  ].join('\n');
}

/** Verified-maker badge and issuer-is-maker label. Renders nothing for a maker with neither. */
export function MakerMarks({ mark, thresholds = VERIFIED_THRESHOLDS, compact }: { mark: MakerMark; thresholds?: Thresholds; compact?: boolean }) {
  if (!mark.verified && !mark.issuer) return null;
  return (
    <span className="inline-flex items-center gap-1">
      {mark.verified && (
        <span title={scoreHint(mark, thresholds)} aria-label="Verified maker" className="inline-flex items-center rounded-full bg-status-success-subtle px-1.5 text-tiny font-medium text-status-success">
          ✓{compact ? '' : ' Verified'}
        </span>
      )}
      {mark.issuer && (
        <span title="The Kaleido curated list names this maker key as the asset’s issuer, or acting for it. The issuer sets the price of a token with no outside market." className="inline-flex items-center rounded-full bg-status-warning-subtle px-1.5 text-tiny font-medium text-status-warning">
          Issuer
        </span>
      )}
    </span>
  );
}

/**
 * Who makes this market and how much has settled. Volume and counterparties come from signed
 * scores (30 days, checked on-chain, self-trades removed), unlike the 24h figures reported by makers.
 */
export function MarketMakers({ rows, sat, trades, ticker, scoresLoaded, thresholds = VERIFIED_THRESHOLDS }: { rows: MarketMakerRow[]; sat: number; trades: number; ticker: string; scoresLoaded: boolean; thresholds?: Thresholds }) {
  return (
    <section aria-label="Makers on this market" className="rounded-2xl bg-surface-card/[0.04] p-4">
      <header className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="text-subhead font-medium">Makers on {ticker}</h2>
        <span className="text-caption text-content-secondary tabular-nums">
          Verified volume, 30 days: <strong className="text-content-primary">{sat.toLocaleString()} sat</strong> · {trades} {trades === 1 ? 'trade' : 'trades'}
        </span>
      </header>
      {rows.length === 0 ? (
        <p className="mt-3 text-caption text-content-tertiary">No makers on this market yet.</p>
      ) : (
        <table className="mt-3 w-full text-caption tabular-nums">
          <thead className="text-tiny text-content-tertiary">
            <tr>
              <th className="py-1 text-left font-normal">Maker</th>
              <th className="py-1 text-right font-normal">Open</th>
              <th className="py-1 text-right font-normal">Trades</th>
              <th className="py-1 text-right font-normal" title="Distinct takers in this asset, as far as the chain shows; a maker trading with itself counts nothing">Counterparties</th>
              <th className="py-1 text-right font-normal">Volume (sat)</th>
              <th className="hidden py-1 text-right font-normal sm:table-cell">Presence</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.maker} className="border-t border-border-subtle">
                <td className="py-1.5">
                  <span className="mr-1.5 font-mono text-tiny text-content-secondary" title={r.maker}>{shortId(r.maker, 6, 4)}</span>
                  <MakerMarks mark={r.mark} thresholds={thresholds} />
                </td>
                <td className="py-1.5 text-right">{r.open}</td>
                <td className="py-1.5 text-right">{r.trades}</td>
                <td className="py-1.5 text-right">{r.counterparties ?? (r.trades ? '—' : 0)}</td>
                <td className="py-1.5 text-right">{r.sat.toLocaleString()}</td>
                <td className="hidden py-1.5 text-right sm:table-cell">{pct(r.mark.score?.presence)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="mt-3 text-tiny text-content-tertiary">
        {scoresLoaded
          ? 'Measured by the Mosaik indexer from the relay and the chain, signed with its key and checked here. Trades a maker made with its own coins are not counted.'
          : 'Maker scores are not available right now; open orders are still listed.'}
      </p>
    </section>
  );
}
