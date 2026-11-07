import clsx from 'clsx';
import { useState } from 'react';
import { useApiState } from './api/client';
import { useRelayFeed } from './api/relay';
import { useRgbP2pBook } from './api/rgbp2p';
import { WebRgbProvider } from './api/webrgb';
import type { Backend } from './api/types';
import { Header } from './components/Header';
import { OrderBook } from './components/OrderBook';
import { MakeOfferForm } from './components/MakeOfferForm';
import { TradeTape } from './components/TradeTape';
import { SwapBoard } from './components/SwapBoard';
import { RelayFeed } from './components/RelayFeed';
import { RgbP2pBook } from './components/RgbP2pBook';
import { WalletButton } from './components/WalletButton';
import { WalletPanel } from './components/WalletPanel';

type View = 'market' | 'wallet' | 'covenant';
const NAV: { id: View; label: string; icon: string }[] = [
  { id: 'market', label: 'Market', icon: 'M3 17l6-6 4 4 8-8M15 7h6v6' },
  { id: 'wallet', label: 'Wallet', icon: 'M3 7h18v12H3zM3 7l3-3h12l3 3M16 13h2' },
  { id: 'covenant', label: 'Covenant DEX', icon: 'M12 3l9 5-9 5-9-5 9-5zM3 13l9 5 9-5' },
];

export function App() {
  return (
    <WebRgbProvider>
      <Shell />
    </WebRgbProvider>
  );
}

function Shell() {
  const [view, setView] = useState<View>('market');
  const book = useRgbP2pBook();
  const live = book.status === 'open';

  return (
    <div className="flex min-h-screen">
      <aside className="sticky top-0 hidden h-screen w-56 shrink-0 flex-col border-r border-ink-500/40 bg-ink-900 p-4 md:flex">
        <Brand />
        <nav className="mt-8 flex flex-col gap-1" aria-label="Main">
          {NAV.map((n) => (
            <NavButton key={n.id} item={n} active={view === n.id} onClick={() => setView(n.id)} />
          ))}
        </nav>
        <div className="mt-auto text-[11px] text-zinc-500">
          <a className="hover:text-mosaic-cyan" href="https://github.com/kaleidoswap/mosaik" target="_blank" rel="noreferrer">Mosaik</a> · KaleidoSwap
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-ink-500/40 bg-ink-800/90 px-4 py-3 backdrop-blur md:px-6">
          <div className="md:hidden"><Brand compact /></div>
          <h1 className="hidden text-base font-semibold md:block">{NAV.find((n) => n.id === view)?.label}</h1>
          <div className="ml-auto flex items-center gap-3">
            <span className="chip bg-ink-600 text-zinc-300 ring-1 ring-ink-500/60" title={book.error ?? 'Order book relay'}>
              <span className={clsx('h-1.5 w-1.5 rounded-full', live ? 'bg-mosaic-mint' : book.status === 'connecting' ? 'animate-pulse bg-mosaic-amber' : 'bg-mosaic-rose')} />
              {live ? 'relay live' : book.status}
            </span>
            <WalletButton />
          </div>
        </header>

        <nav className="flex gap-1 overflow-x-auto border-b border-ink-500/40 px-4 py-2 md:hidden" aria-label="Main">
          {NAV.map((n) => (
            <NavButton key={n.id} item={n} active={view === n.id} onClick={() => setView(n.id)} compact />
          ))}
        </nav>

        <main className="flex-1 p-4 md:p-6">
          {view === 'market' && <RgbP2pBook book={book} />}
          {view === 'wallet' && <WalletPanel />}
          {view === 'covenant' && <CovenantView />}
        </main>
      </div>
    </div>
  );
}

function Brand({ compact }: { compact?: boolean }) {
  return (
    <div className="flex items-center gap-2.5">
      <svg width="28" height="28" viewBox="0 0 28 28" aria-hidden>
        <rect x="2" y="2" width="11" height="11" rx="3" fill="#8FD5EA" />
        <rect x="15" y="2" width="11" height="11" rx="3" fill="#9365FF" />
        <rect x="2" y="15" width="11" height="11" rx="3" fill="#9365FF" />
        <rect x="15" y="15" width="11" height="11" rx="3" fill="#15E99A" />
      </svg>
      {!compact && <span className="text-lg font-semibold tracking-tight">Mosaik</span>}
    </div>
  );
}

function NavButton({ item, active, onClick, compact }: { item: (typeof NAV)[number]; active: boolean; onClick: () => void; compact?: boolean }) {
  return (
    <button
      className={clsx(
        'flex items-center gap-3 rounded-xl px-3 py-2 text-sm transition',
        compact && 'shrink-0',
        active ? 'bg-ink-600 text-white' : 'text-zinc-400 hover:bg-ink-700 hover:text-zinc-200',
      )}
      aria-current={active ? 'page' : undefined}
      onClick={onClick}
      type="button"
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
        <path d={item.icon} />
      </svg>
      {item.label}
    </button>
  );
}

function CovenantView() {
  const [backend, setBackend] = useState<Backend>('liquid');
  const [activeMaker, setActiveMaker] = useState('user');
  const state = useApiState();
  const relay = useRelayFeed();
  return (
    <div className="-mx-4 -my-4 flex flex-col md:-mx-6 md:-my-6">
      <Header
        blockCount={state.data?.block_count}
        backend={backend}
        onBackendChange={setBackend}
        makers={state.data?.makers ?? []}
        activeMaker={activeMaker}
        onMakerChange={setActiveMaker}
        wallet={state.data?.wallets?.[activeMaker]}
        relay={{ status: relay.status, url: relay.url, count: relay.events.length }}
      />
      {state.isLoading && <StatusBar>Loading mosaik-api…</StatusBar>}
      {state.error && (
        <StatusBar variant="error">mosaik-api unreachable: {(state.error as Error).message}</StatusBar>
      )}
      <div className="grid grid-cols-12 gap-4 p-4 md:p-6">
        <div className="col-span-12 flex min-h-0 flex-col gap-4 lg:col-span-8">
          <OrderBook offers={state.data?.offers ?? []} backend={backend} activeTaker={activeMaker} />
          <TradeTape trades={state.data?.trades ?? []} />
        </div>
        <div className="col-span-12 flex min-h-0 flex-col gap-4 lg:col-span-4">
          <MakeOfferForm backend={backend} assets={state.data?.assets ?? []} activeMaker={activeMaker} />
          <SwapBoard />
          <RelayFeed events={relay.events} />
        </div>
      </div>
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
