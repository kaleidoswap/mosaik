import type { Backend } from '../api/types';
import clsx from 'clsx';

interface Props {
  backend: Backend;
  active?: boolean;
  onClick?: () => void;
}

/** Backend pill: a clickable chip that doubles as a selector when given onClick. */
export function BackendBadge({ backend, active, onClick }: Props) {
  const label = backend === 'liquid' ? 'Liquid · Tessera' : 'Arkade · banco';
  const cls =
    backend === 'liquid'
      ? 'bg-mosaic-cyan/15 text-mosaic-cyan ring-mosaic-cyan/30'
      : 'bg-mosaic-violet/15 text-mosaic-violet ring-mosaic-violet/30';

  const base =
    'inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-mono uppercase tracking-wider ring-1 transition';

  if (!onClick) {
    return <span className={clsx(base, cls)}>{label}</span>;
  }
  return (
    <button
      type="button"
      onClick={onClick}
      className={clsx(
        base,
        cls,
        'cursor-pointer hover:brightness-125',
        active ? 'ring-2 ring-offset-1 ring-offset-ink-900' : 'opacity-70 hover:opacity-100',
      )}
    >
      {label}
    </button>
  );
}
