import clsx from 'clsx';
import { useState, type ReactNode } from 'react';

const PATHS = {
  check: 'M5 13l4 4L19 7',
  x: 'M6 6l12 12M18 6L6 18',
  alert: 'M12 9v4M12 17h.01M10.3 3.9L2.4 18a2 2 0 001.7 3h15.8a2 2 0 001.7-3L13.7 3.9a2 2 0 00-3.4 0z',
  info: 'M12 16v-4M12 8h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  external: 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5',
  copy: 'M9 9h10v10H9zM5 15V5h10',
  lock: 'M7 11V8a5 5 0 0110 0v3M5 11h14v9H5z',
  chevron: 'M6 9l6 6 6-6',
  clock: 'M12 7v5l3 2M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
  shield: 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z',
  wallet: 'M3 7h18v12H3zM3 7l3-3h12l3 3M16 13h2',
  swap: 'M7 4v14M7 18l-3-3M7 18l3-3M17 20V6M17 6l-3 3M17 6l3 3',
} as const;
export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16, className }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden className={clsx('shrink-0', className)}>
      <path d={PATHS[name]} />
    </svg>
  );
}

export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" className={clsx('animate-spin shrink-0', className)} aria-hidden>
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeOpacity="0.25" strokeWidth="3" />
      <path d="M21 12a9 9 0 00-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}

/** A value that can be copied whole, shown shortened. */
export function CopyValue({ value, shown, className }: { value: string; shown: ReactNode; className?: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      className={clsx('inline-flex items-center gap-1 rounded font-mono hover:text-content-primary', className)}
      title={`${value}\nClick to copy`}
      aria-label={`Copy ${value}`}
      onClick={() => {
        void navigator.clipboard?.writeText(value).then(() => {
          setDone(true);
          window.setTimeout(() => setDone(false), 1_200);
        }).catch(() => {});
      }}
    >
      {shown}
      <Icon name={done ? 'check' : 'copy'} size={12} className={done ? 'text-status-success' : 'text-content-tertiary'} />
    </button>
  );
}

export function Pill({ tone = 'muted', children, className }: { tone?: 'muted' | 'success' | 'danger' | 'warning' | 'info' | 'primary'; children: ReactNode; className?: string }) {
  const tones = {
    muted: 'bg-surface-card/5 text-content-secondary',
    success: 'bg-status-success-subtle text-status-success',
    danger: 'bg-status-danger-subtle text-status-danger',
    warning: 'bg-status-warning-subtle text-status-warning',
    info: 'bg-status-info-subtle text-status-info',
    primary: 'bg-primary/15 text-primary',
  };
  return <span className={clsx('inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-tiny font-medium', tones[tone], className)}>{children}</span>;
}

export function Notice({ tone, children, action }: { tone: 'warning' | 'danger' | 'info'; children: ReactNode; action?: ReactNode }) {
  const tones = {
    warning: 'border-status-warning/30 bg-status-warning-subtle text-status-warning',
    danger: 'border-status-danger/30 bg-status-danger-subtle text-status-danger',
    info: 'border-status-info/30 bg-status-info-subtle text-status-info',
  };
  return (
    <div role={tone === 'info' ? 'status' : 'alert'} className={clsx('flex items-start gap-2.5 rounded-xl border px-3 py-2.5 text-caption', tones[tone])}>
      <Icon name={tone === 'info' ? 'info' : 'alert'} size={16} className="mt-0.5" />
      <div className="min-w-0 flex-1">{children}</div>
      {action}
    </div>
  );
}

export const btnPrimary =
  'inline-flex items-center justify-center gap-2 rounded-xl bg-primary px-4 py-2.5 text-caption font-bold text-primary-foreground transition hover:bg-primary-emphasis disabled:cursor-not-allowed disabled:opacity-40';
export const btnSecondary =
  'inline-flex items-center justify-center gap-2 rounded-xl bg-surface-elevated px-4 py-2.5 text-caption font-medium text-content-primary transition hover:bg-surface-high disabled:cursor-not-allowed disabled:opacity-40';
