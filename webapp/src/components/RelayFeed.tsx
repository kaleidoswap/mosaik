import type { Event } from 'nostr-tools';
import clsx from 'clsx';
import { shortId } from '../lib/format';

interface Props {
  events: Event[];
}

/**
 * Most-recent-first list of Mosaik relay events. One row per event with a
 * kind chip, the maker pubkey shortened, and a small content preview.
 */
export function RelayFeed({ events }: Props) {
  return (
    <section className="panel flex flex-col min-h-0">
      <header className="panel-header">
        <h2 className="panel-title">Live relay feed</h2>
        <span className="text-[10px] font-mono text-zinc-500">{events.length} buffered</span>
      </header>
      {events.length === 0 ? (
        <div className="p-6 text-center text-sm text-zinc-500">
          No relay events yet. Publish an offer or maker profile to see them flow in.
        </div>
      ) : (
        <ul className="divide-y divide-ink-500/40 overflow-auto">
          {events.map((e) => (
            <li key={e.id} className="px-3 py-2 font-mono text-xs">
              <div className="flex items-center gap-2">
                <KindChip kind={e.kind} />
                <span className="text-zinc-500">{relTime(e.created_at)}</span>
                <span className="ml-auto text-zinc-500" title={e.pubkey}>
                  {shortId(e.pubkey, 6, 4)}
                </span>
              </div>
              <div className="mt-1 truncate text-zinc-400" title={e.id}>
                {summarise(e)}
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function KindChip({ kind }: { kind: number }) {
  const label =
    kind === 30050
      ? 'offer'
      : kind === 30051
        ? 'profile'
        : kind === 30052
          ? 'swap'
          : `${kind}`;
  const cls =
    kind === 30050
      ? 'bg-mosaic-cyan/15 text-mosaic-cyan ring-mosaic-cyan/30'
      : kind === 30051
        ? 'bg-mosaic-amber/15 text-mosaic-amber ring-mosaic-amber/30'
        : kind === 30052
          ? 'bg-mosaic-violet/15 text-mosaic-violet ring-mosaic-violet/30'
          : 'bg-ink-500/60 text-zinc-300 ring-ink-400/40';
  return (
    <span
      className={clsx(
        'inline-flex items-center rounded-full px-1.5 py-px text-[9px] font-mono uppercase tracking-wider ring-1',
        cls,
      )}
    >
      {label}
    </span>
  );
}

function relTime(unix: number): string {
  const now = Math.floor(Date.now() / 1000);
  const dt = Math.max(0, now - unix);
  if (dt < 60) return `${dt}s`;
  if (dt < 3600) return `${Math.floor(dt / 60)}m`;
  return `${Math.floor(dt / 3600)}h`;
}

/** Cheap content preview — try to surface the most-informative field per kind. */
function summarise(e: Event): string {
  try {
    const body = JSON.parse(e.content);
    if (e.kind === 30050 && body?.offer) {
      const o = body.offer;
      const want =
        o?.kind?.backend === 'banco'
          ? `${o.kind.want_amount} ${o.kind.want_asset ?? 'BTC'}`
          : `${o?.kind?.amount_b} of asset`;
      return `${o?.amount_a} ${o?.asset_a} → ${want}`;
    }
    if (e.kind === 30051 && body?.name) {
      const badges = (body.badges ?? []).join(', ');
      return badges ? `${body.name} (${badges})` : body.name;
    }
    if (e.kind === 30052 && body?.state?.state) {
      return `${body.swap_id ?? '<no id>'} — ${body.state.state}`;
    }
    return JSON.stringify(body).slice(0, 80);
  } catch {
    return e.content.slice(0, 80);
  }
}
