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
import { Market } from './components/market/Market';
import { SwapsProvider } from './api/swaps';
import { WalletButton } from './components/WalletButton';
import { WalletPanel } from './components/WalletPanel';

type View = 'market' | 'wallet' | 'covenant';
const NAV: { id: View; label: string; icon: string }[] = [
  { id: 'market', label: 'Market', icon: 'M3 17l6-6 4 4 8-8M15 7h6v6' },
  { id: 'wallet', label: 'Wallet', icon: 'M3 7h18v12H3zM3 7l3-3h12l3 3M16 13h2' },
];
/** The covenant (Liquid) DEX stays reachable, but it is not the main product of this page. */
const MORE: (typeof NAV)[number] = { id: 'covenant', label: 'Covenant DEX', icon: 'M12 3l9 5-9 5-9-5 9-5zM3 13l9 5 9-5' };
const TITLES: Record<View, string> = { market: 'Market', wallet: 'Wallet', covenant: 'Covenant DEX' };

export function App() {
  return (
    <WebRgbProvider>
      <SwapsProvider>
        <Shell />
      </SwapsProvider>
    </WebRgbProvider>
  );
}

function Shell() {
  const [view, setView] = useState<View>('market');
  const book = useRgbP2pBook();
  const live = book.status === 'open';

  return (
    <div className="flex min-h-screen">
      <a href="#main" className="sr-only z-[60] rounded-lg bg-surface-overlay px-3 py-2 text-caption font-semibold focus:not-sr-only focus:fixed focus:left-3 focus:top-3">Skip to content</a>
      <aside className="sticky top-0 hidden h-screen w-56 shrink-0 flex-col border-r border-border-subtle bg-surface-base p-4 md:flex">
        <Brand />
        <nav className="mt-8 flex flex-col gap-1" aria-label="Main">
          {NAV.map((n) => (
            <NavButton key={n.id} item={n} active={view === n.id} onClick={() => setView(n.id)} />
          ))}
        </nav>
        <div className="mt-auto flex flex-col gap-3">
          <div className="border-t border-border-subtle pt-3">
            <div className="px-3 pb-1 text-xxs uppercase tracking-[0.18em] text-content-tertiary">Experimental</div>
            <NavButton item={MORE} active={view === MORE.id} onClick={() => setView(MORE.id)} muted />
          </div>
        </div>
        <div className="mt-3 text-[11px] text-content-tertiary">
          <a className="hover:text-primary" href="https://github.com/kaleidoswap/mosaik" target="_blank" rel="noreferrer">Mosaik</a> · KaleidoSwap
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-border-subtle bg-surface-base/90 px-4 py-3 backdrop-blur md:px-6">
          <div className="md:hidden"><Brand compact /></div>
          <h1 className="hidden text-base font-semibold md:block">{TITLES[view]}</h1>
          <div className="ml-auto flex items-center gap-3">
            <span className="inline-flex items-center gap-1.5 rounded-full bg-surface-card/5 px-2.5 py-1 text-tiny text-content-secondary" title={book.error ?? 'Order book relay'}>
              <span className={clsx('h-1.5 w-1.5 rounded-full', live ? 'bg-status-success' : book.status === 'connecting' ? 'animate-pulse bg-status-warning' : 'bg-status-danger')} />
              {live ? 'relay live' : book.status}
            </span>
            <WalletButton />
          </div>
        </header>

        <nav className="flex gap-1 overflow-x-auto border-b border-border-subtle px-4 py-2 md:hidden" aria-label="Main">
          {[...NAV, MORE].map((n) => (
            <NavButton key={n.id} item={n} active={view === n.id} onClick={() => setView(n.id)} compact />
          ))}
        </nav>

        <main id="main" tabIndex={-1} className="flex-1 p-4 outline-none md:p-6">
          {view === 'market' && <Market book={book} />}
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

function NavButton({ item, active, onClick, compact, muted }: { item: (typeof NAV)[number]; active: boolean; onClick: () => void; compact?: boolean; muted?: boolean }) {
  return (
    <button
      className={clsx(
        'flex items-center gap-3 rounded-xl px-3 py-2 text-sm transition',
        compact && 'shrink-0',
        active ? 'bg-surface-elevated text-content-primary' : clsx('hover:bg-surface-raised hover:text-content-primary', muted ? 'text-content-tertiary' : 'text-content-secondary'),
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
