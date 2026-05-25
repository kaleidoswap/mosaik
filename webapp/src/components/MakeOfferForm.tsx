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

  const makeOffer = useMakeOffer();
  const fundWallet = useFundWallet();

  const allAssets = ['BTC', ...assets.filter((a) => a !== 'BTC')];
  const disabled = backend === 'banco';

  function submit(e: React.FormEvent) {
    e.preventDefault();
    if (lock === want) return;
    makeOffer.mutate({
      lock,
      want,
      amount_a: Number(amountA),
      amount_b: Number(amountB),
      timeout: Number(timeout_),
      maker: activeMaker,
    });
  }

  return (
    <section className="panel">
      <header className="panel-header">
        <h2 className="panel-title">Make offer</h2>
        <BackendBadge backend={backend} />
      </header>

      {disabled && (
        <div className="border-b border-mosaic-violet/30 bg-mosaic-violet/10 px-4 py-2.5 text-xs text-mosaic-violet">
          Arkade banco offers travel through <code>banco-proxy</code> + the CLI today; the
          webapp wires them up in the next iteration. Switch to Liquid to post here.
        </div>
      )}

      <form onSubmit={submit} className="grid grid-cols-2 gap-3 p-4">
        <div className="col-span-1">
          <label className="label">Lock asset</label>
          <select
            className="input"
            value={lock}
            onChange={(e) => setLock(e.target.value)}
            disabled={disabled}
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
            disabled={disabled}
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
            disabled={disabled}
          />
        </div>
        <div>
          <label className="label">Amount B (want, raw)</label>
          <input
            className="input font-mono"
            value={amountB}
            onChange={(e) => setAmountB(e.target.value)}
            inputMode="numeric"
            disabled={disabled}
          />
        </div>

        <div className="col-span-2">
          <label className="label">
            {backend === 'liquid'
              ? 'Refund block-height timeout'
              : 'Cancel delay (unix seconds)'}
          </label>
          <input
            className="input font-mono"
            value={timeout_}
            onChange={(e) => setTimeout_(e.target.value)}
            inputMode="numeric"
            disabled={disabled}
          />
        </div>

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
            disabled={disabled || makeOffer.isPending || lock === want}
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
