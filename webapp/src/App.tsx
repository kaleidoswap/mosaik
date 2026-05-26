import { useState } from 'react';
import { useApiState } from './api/client';
import type { Backend } from './api/types';
import { Header } from './components/Header';
import { OrderBook } from './components/OrderBook';
import { MakeOfferForm } from './components/MakeOfferForm';
import { TradeTape } from './components/TradeTape';
import { SwapBoard } from './components/SwapBoard';

export function App() {
  const [backend, setBackend] = useState<Backend>('liquid');
  const [activeMaker, setActiveMaker] = useState('user');

  const state = useApiState();

  return (
    <div className="flex min-h-screen flex-col">
      <Header
        blockCount={state.data?.block_count}
        backend={backend}
        onBackendChange={setBackend}
        makers={state.data?.makers ?? []}
        activeMaker={activeMaker}
        onMakerChange={setActiveMaker}
        wallet={state.data?.wallets?.[activeMaker]}
      />

      {state.isLoading && <StatusBar>Loading mosaik-api…</StatusBar>}
      {state.error && (
        <StatusBar variant="error">
          mosaik-api unreachable: {(state.error as Error).message}
        </StatusBar>
      )}

      <main className="mx-auto grid w-full max-w-screen-2xl flex-1 grid-cols-12 gap-4 p-4">
        <div className="col-span-12 lg:col-span-8 flex flex-col gap-4 min-h-0">
          <OrderBook
            offers={state.data?.offers ?? []}
            backend={backend}
            activeTaker={activeMaker}
          />
          <TradeTape trades={state.data?.trades ?? []} />
        </div>
        <div className="col-span-12 lg:col-span-4 flex flex-col gap-4 min-h-0">
          <MakeOfferForm
            backend={backend}
            assets={state.data?.assets ?? []}
            activeMaker={activeMaker}
          />
          <SwapBoard />
        </div>
      </main>

      <Footer relay={state.data?.relay} />
    </div>
  );
}

function StatusBar({
  variant = 'info',
  children,
}: {
  variant?: 'info' | 'error';
  children: React.ReactNode;
}) {
  const cls =
    variant === 'error'
      ? 'border-mosaic-rose/30 bg-mosaic-rose/10 text-mosaic-rose'
      : 'border-mosaic-cyan/30 bg-mosaic-cyan/10 text-mosaic-cyan';
  return (
    <div className={`border-b px-6 py-2 text-xs font-mono ${cls}`}>{children}</div>
  );
}

function Footer({ relay }: { relay?: string }) {
  return (
    <footer className="mx-auto flex w-full max-w-screen-2xl items-center justify-between gap-4 px-6 py-3 text-[11px] font-mono uppercase tracking-wider text-zinc-500">
      <span>
        covenant&nbsp;DEX&nbsp;platform&nbsp;·&nbsp;
        <a
          href="https://github.com/kaleidoswap/mosaik"
          className="hover:text-mosaic-cyan"
          target="_blank"
          rel="noreferrer"
        >
          mosaik
        </a>
      </span>
      {relay && (
        <span>
          relay&nbsp;<span className="text-zinc-300">{relay}</span>
        </span>
      )}
    </footer>
  );
}
