import clsx from 'clsx';
import type { RelayStatus } from '../api/relay';

interface Props {
  status: RelayStatus;
  url: string;
  count: number;
}

/**
 * Header-bar chip showing the Mosaik relay subscription state and the
 * running event count. Dot-pulse colour reflects status:
 *   connecting → amber (pulsing)
 *   open       → mint (steady)
 *   closed     → zinc
 *   error      → rose
 */
export function RelayStatusChip({ status, url, count }: Props) {
  const dotColor: Record<RelayStatus, string> = {
    connecting: 'bg-mosaic-amber animate-pulse',
    open: 'bg-mosaic-mint',
    closed: 'bg-zinc-500',
    error: 'bg-mosaic-rose',
  };
  const label: Record<RelayStatus, string> = {
    connecting: 'connecting',
    open: 'live',
    closed: 'closed',
    error: 'error',
  };

  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full bg-ink-500/60 px-2 py-0.5 text-[10px] font-mono uppercase tracking-wider text-zinc-300 ring-1 ring-ink-400/40"
      title={`relay ${url} — ${count} event${count === 1 ? '' : 's'} buffered`}
    >
      <span className={clsx('h-1.5 w-1.5 rounded-full', dotColor[status])} />
      relay {label[status]} · {count}
    </span>
  );
}
