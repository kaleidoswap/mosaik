import type { Backend, OfferView } from '../api/types';
import { useMakerProfiles, useReclaim, useTakeOffer } from '../api/client';
import { fmtSats, fmtRate, shortId } from '../lib/format';
import { BackendBadge } from './BackendBadge';
import { MakerProfileChip } from './MakerProfileChip';
import clsx from 'clsx';

interface Props {
  offers: OfferView[];
  backend: Backend;
  activeTaker: string;
}

export function OrderBook({ offers, backend, activeTaker }: Props) {
  const takeOffer = useTakeOffer();
  const reclaim = useReclaim();
  const profiles = useMakerProfiles();
  const profileByLabel = new Map(
    (profiles.data?.profiles ?? []).map((p) => [p.label ?? '', p]),
  );

  return (
    <section className="panel flex flex-col min-h-0">
      <header className="panel-header">
        <h2 className="panel-title">Order book</h2>
        <span className="text-[10px] font-mono text-zinc-500">
          {offers.length} live · {backend}
        </span>
      </header>

      {offers.length === 0 ? (
        <div className="p-8 text-center text-sm text-zinc-500">
          No offers yet. Post one from <em>Make offer</em> →
        </div>
      ) : (
        <div className="overflow-auto">
          <table className="w-full text-sm">
            <thead className="text-[10px] font-mono uppercase tracking-wider text-zinc-500">
              <tr className="border-b border-ink-500/60">
                <th className="px-3 py-2 text-left">Pair</th>
                <th className="px-3 py-2 text-right">Sells</th>
                <th className="px-3 py-2 text-right">Wants</th>
                <th className="px-3 py-2 text-right">Rate</th>
                <th className="px-3 py-2 text-left">Maker</th>
                <th className="px-3 py-2 text-left">UTXO</th>
                <th className="px-3 py-2 text-right">Action</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {offers.map((o) => {
                const isOwn = o.maker === activeTaker;
                return (
                  <tr
                    key={o.index}
                    className="border-b border-ink-500/30 hover:bg-ink-500/20"
                  >
                    <td className="px-3 py-2">
                      <span className="text-zinc-200">
                        {o.lock_name}
                        <span className="text-zinc-500">/</span>
                        {o.want_name}
                      </span>
                      {o.backend && (
                        <span className="ml-2 align-middle">
                          <BackendBadge backend={o.backend} />
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-2 text-right text-mosaic-cyan">
                      {fmtSats(o.amount_a)}
                    </td>
                    <td className="px-3 py-2 text-right text-mosaic-violet">
                      {fmtSats(o.amount_b)}
                    </td>
                    <td className="px-3 py-2 text-right text-zinc-300">
                      {fmtRate(o.amount_a, o.amount_b)}
                    </td>
                    <td className="px-3 py-2 text-zinc-300">
                      <span className="mr-1.5">{o.maker_name}</span>
                      <MakerProfileChip profile={profileByLabel.get(o.maker)} />
                      {isOwn && <span className="ml-1 pill">you</span>}
                    </td>
                    <td className="px-3 py-2 text-zinc-500">{shortId(o.outpoint, 8, 4)}</td>
                    <td className="px-3 py-2 text-right">
                      <div className="inline-flex gap-1.5">
                        {!isOwn ? (
                          <button
                            className="btn-primary !py-1 !px-2 !text-xs"
                            disabled={takeOffer.isPending}
                            onClick={() =>
                              takeOffer.mutate({ index: o.index, taker: activeTaker })
                            }
                          >
                            Take
                          </button>
                        ) : (
                          <button
                            className="btn-secondary !py-1 !px-2 !text-xs"
                            disabled={reclaim.isPending}
                            onClick={() => reclaim.mutate(o.index)}
                            title="Sweep via the keyless REFUND path"
                          >
                            Reclaim
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {(takeOffer.error || reclaim.error) && (
        <div className="border-t border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-mosaic-rose">
          {(takeOffer.error ?? reclaim.error)?.message}
        </div>
      )}
      {takeOffer.data && (
        <ResultStrip data={takeOffer.data} />
      )}
    </section>
  );
}

function ResultStrip({ data }: { data: NonNullable<ReturnType<typeof useTakeOffer>['data']> }) {
  // Distinguish honest fill / covenant-rejected cheat / covenant bug.
  if ('rejected' in data && data.rejected) {
    return (
      <div className="border-t border-mosaic-mint/30 bg-mosaic-mint/10 px-3 py-2 text-xs text-mosaic-mint">
        Covenant rejected the spend (as expected): {data.reason}
      </div>
    );
  }
  if ('covenant_bug' in data && data.covenant_bug) {
    return (
      <div className="border-t border-mosaic-rose/40 bg-mosaic-rose/15 px-3 py-2 text-xs text-mosaic-rose">
        ⚠ Covenant bug: fraudulent settlement broadcast — txid {data.txid}
      </div>
    );
  }
  return (
    <div className={clsx(
      'border-t border-mosaic-cyan/30 bg-mosaic-cyan/10 px-3 py-2 text-xs text-mosaic-cyan',
    )}>
      Settled — txid {shortId('txid' in data ? data.txid : '', 12, 8)}
    </div>
  );
}
