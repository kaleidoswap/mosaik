import { useState } from 'react';
import type { Backend } from '../api/types';
import { useMakeOffer, useFundWallet } from '../api/client';
import { BackendBadge } from './BackendBadge';

interface Props {
  backend: Backend;
  assets: string[];
  activeMaker: string;
}

export function MakeOfferForm({ backend, assets, activeMaker }: Props) {
  const [lock, setLock] = useState('BTC');
  const [want, setWant] = useState(assets[0] ?? 'USDT');
  const [amountA, setAmountA] = useState('1000000');
  const [amountB, setAmountB] = useState('500000');
  const [timeout_, setTimeout_] = useState('500');
  const [partialFill, setPartialFill] = useState(false);

  const makeOffer = useMakeOffer();
  const fundWallet = useFundWallet();

  const allAssets = ['BTC', ...assets.filter((a) => a !== 'BTC')];
  const isBanco = backend === 'banco';

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (lock === want) return;
    // For banco the `timeout` input is interpreted as unix-seconds
    // cancel_delay (defaulting to now+2h if unset / 0).
    const cancelDelay = (() => {
      const n = Number(timeout_);
      if (isBanco && (!Number.isFinite(n) || n < 1_000_000)) {
        return Math.floor(Date.now() / 1000) + 7200;
      }
      return n;
    })();
    makeOffer.mutate({
      backend,
      lock,
      want,
      amount_a: Number(amountA),
      amount_b: Number(amountB),
      timeout: isBanco ? cancelDelay : Number(timeout_),
      maker: activeMaker,
      ...(isBanco ? { partial_fill: partialFill } : {}),
    });
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2 className="panel-title">Make offer</h2>
        <BackendBadge backend={backend} />
      </header>

      {isBanco && (
        <div className="border-b border-mosaic-violet/30 bg-mosaic-violet/10 px-4 py-2 text-[11px] text-mosaic-violet/90">
          Banco offers route through <code>banco-proxy</code>. Requires
          <code className="mx-1">BANCO_WALLET_SEED_{activeMaker.toUpperCase()}</code>
          on the proxy and a live arkd.
        </div>
      )}

      <form onSubmit={submit} className="grid grid-cols-2 gap-3 p-4">
        <div className="col-span-1">
          <label className="label">Lock asset</label>
          <select
            className="input"
            value={lock}
            onChange={(e) => setLock(e.target.value)}
          >
            {allAssets.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>
        <div className="col-span-1">
          <label className="label">Want asset</label>
          <select
            className="input"
            value={want}
            onChange={(e) => setWant(e.target.value)}
          >
            {allAssets.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="label">Amount A (lock, raw)</label>
          <input
            className="input font-mono"
            value={amountA}
            onChange={(e) => setAmountA(e.target.value)}
            inputMode="numeric"
          />
        </div>
        <div>
          <label className="label">Amount B (want, raw)</label>
          <input
            className="input font-mono"
            value={amountB}
            onChange={(e) => setAmountB(e.target.value)}
            inputMode="numeric"
          />
        </div>

        <div className="col-span-2">
          <label className="label">
            {isBanco
              ? 'Cancel delay (unix seconds; 0 = default now + 2h)'
              : 'Refund block-height timeout'}
          </label>
          <input
            className="input font-mono"
            value={timeout_}
            onChange={(e) => setTimeout_(e.target.value)}
            inputMode="numeric"
          />
        </div>

        {isBanco && (
          <label className="col-span-2 flex items-center gap-2 text-xs text-zinc-300">
            <input
              type="checkbox"
              checked={partialFill}
              onChange={(e) => setPartialFill(e.target.checked)}
              className="h-3.5 w-3.5 accent-mosaic-violet"
            />
            Accept partial fills
          </label>
        )}

        <div className="col-span-2 flex items-center justify-between gap-2 pt-1">
          <button
            type="button"
            className="btn-secondary"
            onClick={() => fundWallet.mutate(activeMaker)}
            disabled={fundWallet.isPending}
            title="Treasury → active wallet: 1 L-BTC + 100 of each test asset"
          >
            Fund {activeMaker}
          </button>
          <button
            type="submit"
            className="btn-primary"
            disabled={makeOffer.isPending || lock === want}
          >
            {makeOffer.isPending ? 'Funding…' : 'Publish covenant'}
          </button>
        </div>

        {(makeOffer.error || fundWallet.error) && (
          <p className="col-span-2 text-xs text-mosaic-rose">
            {(makeOffer.error ?? fundWallet.error)?.message}
          </p>
        )}
      </form>
    </section>
  );
}
