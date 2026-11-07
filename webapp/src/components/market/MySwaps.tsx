import clsx from 'clsx';
import { useWebRgb } from '../../api/webrgb';
import { isTerminal, useSwaps, type SwapRecord } from '../../api/swaps';
import { fmtUnits } from '../../lib/units';
import { Pill, Spinner } from '../ui';

const LABEL: Record<SwapRecord['state'], { text: string; tone: 'info' | 'success' | 'danger' | 'warning' }> = {
  awaiting_maker: { text: 'Waiting for maker', tone: 'info' },
  signing: { text: 'Signing', tone: 'info' },
  broadcast: { text: 'Confirming', tone: 'info' },
  settled: { text: 'Complete', tone: 'success' },
  failed: { text: 'Failed', tone: 'danger' },
  expired: { text: 'Expired', tone: 'warning' },
};

const ago = (ms: number): string => {
  const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86_400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86_400)}d ago`;
};

/** Swaps this page started, newest first. Their state is the wallet's; this only remembers the ids. */
export function MySwaps({ onOpen }: { onOpen: (r: SwapRecord) => void }) {
  const { swaps, clearFinished } = useSwaps();
  const w = useWebRgb();
  const canClear = swaps.some((r) => isTerminal(r.state));
  return (
    <section aria-label="My swaps" className="rounded-2xl bg-surface-card/[0.04] p-4">
      <header className="flex items-baseline justify-between pb-2">
        <h2 className="text-subhead font-medium">My swaps</h2>
        {canClear && <button type="button" className="text-tiny text-content-tertiary hover:text-content-primary" onClick={clearFinished}>Clear finished</button>}
      </header>
      {swaps.length === 0 ? (
        <p className="py-3 text-caption text-content-tertiary">Swaps you start on this page show up here and keep updating, even after a reload.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {swaps.map((r) => {
            const l = LABEL[r.state];
            const buy = r.order.side === 'sell';
            return (
              <li key={r.swapId}>
                <button type="button" onClick={() => onOpen(r)} className="flex w-full items-center gap-3 rounded-xl px-2 py-2 text-left hover:bg-surface-elevated">
                  <span className={clsx('flex h-8 w-8 shrink-0 items-center justify-center rounded-full text-tiny font-bold', buy ? 'bg-status-success-subtle text-status-success' : 'bg-status-danger-subtle text-status-danger')}>{buy ? 'B' : 'S'}</span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-caption font-medium">{buy ? 'Buy' : 'Sell'} {fmtUnits(r.order.amount, r.order.precision)} {r.order.ticker ?? 'asset'}</span>
                    <span className="block text-tiny text-content-tertiary">{r.order.priceSat.toLocaleString()} sat · {ago(r.startedAt)}</span>
                  </span>
                  <Pill tone={l.tone}>{!isTerminal(r.state) && <Spinner size={10} />}{l.text}</Pill>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {swaps.some((r) => !isTerminal(r.state)) && w.status !== 'connected' && (
        <p className="mt-2 text-tiny text-status-warning">Connect your wallet to refresh swaps that are still running.</p>
      )}
    </section>
  );
}
