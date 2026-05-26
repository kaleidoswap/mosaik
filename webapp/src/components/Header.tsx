import type { Backend, Maker, WalletSnapshot } from '../api/types';
import type { RelayStatus } from '../api/relay';
import { BackendBadge } from './BackendBadge';
import { RelayStatusChip } from './RelayStatusChip';
import { fmtSats } from '../lib/format';

interface Props {
  blockCount?: number;
  backend: Backend;
  onBackendChange: (b: Backend) => void;
  makers: Maker[];
  activeMaker: string;
  onMakerChange: (m: string) => void;
  wallet?: WalletSnapshot;
  relay: { status: RelayStatus; url: string; count: number };
}

export function Header({
  blockCount,
  backend,
  onBackendChange,
  makers,
  activeMaker,
  onMakerChange,
  wallet,
  relay,
}: Props) {
  return (
    <header className="border-b border-ink-500/60 bg-ink-800/60 backdrop-blur">
      <div className="mx-auto flex max-w-screen-2xl items-center justify-between gap-6 px-6 py-3">
        {/* Brand */}
        <div className="flex items-center gap-3">
          <Logo />
          <div className="flex flex-col leading-tight">
            <span className="text-sm font-semibold tracking-wide">Mosaik</span>
            <span className="text-[10px] uppercase tracking-widest text-zinc-500">
              covenant&nbsp;dex&nbsp;platform
            </span>
          </div>
        </div>

        {/* Backend selector */}
        <div className="flex items-center gap-2">
          <span className="label !mb-0">Backend</span>
          <BackendBadge
            backend="liquid"
            active={backend === 'liquid'}
            onClick={() => onBackendChange('liquid')}
          />
          <BackendBadge
            backend="banco"
            active={backend === 'banco'}
            onClick={() => onBackendChange('banco')}
          />
        </div>

        {/* Right cluster */}
        <div className="flex items-center gap-4">
          <RelayStatusChip status={relay.status} url={relay.url} count={relay.count} />
          {blockCount !== undefined && (
            <span className="pill" title="Chain tip">
              block {blockCount}
            </span>
          )}
          <select
            className="input !py-1 !pr-7 !w-auto"
            value={activeMaker}
            onChange={(e) => onMakerChange(e.target.value)}
            aria-label="Active wallet"
          >
            {makers.map((m) => (
              <option key={m.label} value={m.label}>
                {m.name}
              </option>
            ))}
          </select>
          {wallet && (
            <div className="flex flex-col text-right leading-tight">
              <span className="text-[10px] uppercase tracking-widest text-zinc-500">
                balance
              </span>
              <span className="font-mono text-sm text-mosaic-cyan">
                {fmtSats(wallet.lbtc_sats)} sats
              </span>
            </div>
          )}
        </div>
      </div>
    </header>
  );
}

function Logo() {
  return (
    <svg
      width="28"
      height="28"
      viewBox="0 0 32 32"
      fill="none"
      className="drop-shadow-[0_0_8px_rgba(94,231,223,0.4)]"
    >
      <rect x="2" y="2" width="12" height="12" rx="2" fill="#5ee7df" />
      <rect x="18" y="2" width="12" height="12" rx="2" fill="#a78bfa" />
      <rect x="2" y="18" width="12" height="12" rx="2" fill="#a78bfa" />
      <rect x="18" y="18" width="12" height="12" rx="2" fill="#5ee7df" />
    </svg>
  );
}
