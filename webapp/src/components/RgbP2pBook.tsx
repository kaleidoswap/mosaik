import clsx from 'clsx';
import { useMemo, useState } from 'react';
import { canTakeP2p } from '../api/rgbWallet';
import { unitPriceSat, useRgbP2pBook, type RgbOffer } from '../api/rgbp2p';
import { shortId } from '../lib/format';

const fmtUnits = (amount: number, precision: number) =>
  (amount / 10 ** precision).toLocaleString(undefined, { maximumFractionDigits: precision });

const fmtLeft = (expiry: number, now: number) => {
  const s = Math.max(0, expiry - now);
  if (s >= 86_400) return `${Math.floor(s / 86_400)}d`;
  if (s >= 3_600) return `${Math.floor(s / 3_600)}h`;
  return `${Math.max(1, Math.floor(s / 60))}m`;
};

/**
 * The RGB peer-to-peer order book: what makers are offering right now, read from the relay. The
 * asset's name is the maker's own claim; the asset id under it is what identifies it.
 */
export function RgbP2pBook() {
  const book = useRgbP2pBook();
  const [network, setNetwork] = useState<string>();
  const now = Math.floor(Date.now() / 1000);
  const takeable = canTakeP2p();

  const networks = useMemo(() => [...new Set(book.offers.map((o) => o.network))].sort(), [book.offers]);
  const shown = network ?? networks[0];
  const offers = book.offers.filter((o) => o.network === shown);
  const sells = offers.filter((o) => o.side === 'sell').sort(byUnit(1));
  const buys = offers.filter((o) => o.side === 'buy').sort(byUnit(-1));

  return (
    <section className="panel flex flex-col min-h-0">
      <header className="panel-header">
        <h2 className="panel-title">RGB peer-to-peer</h2>
        <div className="flex items-center gap-2">
          {networks.length > 1 &&
            networks.map((n) => (
              <button
                key={n}
                className={clsx(
                  'rounded px-2 py-0.5 text-[10px] font-mono ring-1',
                  n === shown ? 'bg-mosaic-cyan/15 text-mosaic-cyan ring-mosaic-cyan/30' : 'text-zinc-500 ring-ink-500/40',
                )}
                onClick={() => setNetwork(n)}
                type="button"
              >
                {n}
              </button>
            ))}
          <span className="text-[10px] font-mono text-zinc-500">
            {book.status === 'open' ? `${offers.length} orders` : book.status}
          </span>
        </div>
      </header>

      {book.error && <div className="px-3 py-2 text-xs text-red-400">Relay: {book.error}</div>}

      {!takeable && (
        <div className="border-b border-ink-500/40 px-3 py-2 text-xs text-zinc-400">
          Read only. Taking an order needs the KaleidoSwap browser extension, which signs the swap with
          your wallet. The maker must be online to complete it.
        </div>
      )}

      {offers.length === 0 ? (
        <div className="p-6 text-center text-sm text-zinc-500">
          {book.status === 'connecting' ? 'Connecting to the relay…' : 'No open orders on this network.'}
        </div>
      ) : (
        <div className="grid grid-cols-1 divide-y divide-ink-500/40 overflow-auto md:grid-cols-2 md:divide-x md:divide-y-0">
          <Side title="Sellers (you buy)" offers={sells} now={now} takeable={takeable} tone="text-mosaic-cyan" />
          <Side title="Buyers (you sell)" offers={buys} now={now} takeable={takeable} tone="text-mosaic-amber" />
        </div>
      )}
    </section>
  );
}

const byUnit = (dir: 1 | -1) => (a: RgbOffer, b: RgbOffer) =>
  dir * ((unitPriceSat(a) ?? Infinity) - (unitPriceSat(b) ?? Infinity));

function Side({
  title,
  offers,
  now,
  takeable,
  tone,
}: {
  title: string;
  offers: RgbOffer[];
  now: number;
  takeable: boolean;
  tone: string;
}) {
  return (
    <div className="min-w-0">
      <div className={clsx('px-3 py-1.5 text-[10px] uppercase tracking-wider', tone)}>{title}</div>
      {offers.length === 0 ? (
        <div className="px-3 py-3 text-xs text-zinc-600">none</div>
      ) : (
        <ul className="divide-y divide-ink-500/30">
          {offers.map((o) => {
            const unit = unitPriceSat(o);
            return (
              <li key={o.key} className="px-3 py-2 font-mono text-xs">
                <div className="flex items-baseline gap-2">
                  <span className="text-zinc-200">{o.ticker ?? 'asset'}</span>
                  <span className="text-zinc-400">{fmtUnits(o.amount, o.precision)}</span>
                  <span className="ml-auto text-zinc-200">{o.priceSat.toLocaleString()} sat</span>
                </div>
                <div className="mt-0.5 flex items-center gap-2 text-[10px] text-zinc-500">
                  <span title={o.assetId}>{shortId(o.assetId.replace('rgb:', ''), 6, 4)}</span>
                  <span>{unit ? `${unit.toLocaleString(undefined, { maximumFractionDigits: 2 })} sat/unit` : ''}</span>
                  <span title={o.maker}>maker {shortId(o.maker, 5, 3)}</span>
                  <span className="ml-auto">{fmtLeft(o.expiry, now)} left</span>
                  <button
                    className="rounded px-2 py-0.5 ring-1 ring-ink-500/40 disabled:opacity-40"
                    disabled={!takeable}
                    title={takeable ? 'Review and take' : 'Needs the browser extension'}
                    type="button"
                  >
                    Take
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
