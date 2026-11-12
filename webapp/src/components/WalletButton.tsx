import clsx from 'clsx';
import { useEffect, useRef, useState } from 'react';
import { useWebRgb } from '../api/webrgb';
import { fmtSats } from '../lib/format';

/** Header control: sign in with the WebRGB wallet, then show its balance and a way out. */
export function WalletButton() {
  const w = useWebRgb();
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener('mousedown', close);
    return () => document.removeEventListener('mousedown', close);
  }, [open]);

  if (w.status === 'absent') {
    return (
      <a className="btn-secondary" href="#learn" title="The extension holds your keys and signs swaps">
        Set up wallet
      </a>
    );
  }
  if (w.status !== 'connected') {
    return (
      <div className="flex items-center gap-2">
        {w.error && <span className="hidden text-xs text-status-danger md:inline">{w.error}</span>}
        <button className="btn-primary" disabled={w.status === 'connecting'} onClick={() => void w.connect()} type="button">
          {w.status === 'connecting' ? 'Approve in the wallet…' : `Connect ${w.walletName ?? 'wallet'}`}
        </button>
      </div>
    );
  }
  return (
    <div className="relative" ref={box}>
      <button className="btn-secondary !gap-2" onClick={() => setOpen((v) => !v)} type="button" aria-expanded={open}>
        <span className="h-2 w-2 rounded-full bg-status-success" />
        <span className="font-mono text-xs">{w.balance ? `${fmtSats(w.balance.spendable)} sat` : 'connected'}</span>
        <span className="hidden text-xs text-content-secondary sm:inline">{w.walletName ?? 'Wallet'}</span>
      </button>
      {open && (
        <div className="absolute right-0 z-20 mt-2 w-64 rounded-xl border border-border-default bg-surface-overlay p-3 shadow-xl">
          <div className="text-[11px] uppercase tracking-wider text-content-tertiary">Connected wallet</div>
          <div className="mt-1 text-sm">{w.walletName ?? 'WebRGB wallet'}</div>
          <div className={clsx('mt-0.5 text-xs', 'text-content-secondary')}>Network {w.walletLabel ?? w.network ?? 'unknown'}</div>
          <div className="mt-3 flex gap-2">
            <button className="btn-secondary !py-1 flex-1" onClick={() => void w.refresh()} type="button">Refresh</button>
            <button className="btn-secondary !py-1 flex-1" onClick={() => { w.disconnect(); setOpen(false); }} type="button">Disconnect</button>
          </div>
        </div>
      )}
    </div>
  );
}
