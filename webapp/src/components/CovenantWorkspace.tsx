import { useState } from 'react';
import { useApiState } from '../api/client';
import { useRelayFeed } from '../api/relay';
import type { Backend } from '../api/types';
import { Header } from './Header';
import { OrderBook } from './OrderBook';
import { MakeOfferForm } from './MakeOfferForm';
import { TradeTape } from './TradeTape';
import { SwapBoard } from './SwapBoard';
import { RelayFeed } from './RelayFeed';
export default function CovenantWorkspace({ initialBackend = 'liquid' }: { initialBackend?: Backend }) {
  const [backend, setBackend] = useState<Backend>(initialBackend);
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
