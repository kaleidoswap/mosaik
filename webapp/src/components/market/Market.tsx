import clsx from 'clsx';
import { useMemo, useState } from 'react';
import type { RgbBookState, RgbOffer } from '../../api/rgbp2p';
import { snapshotOf, type OrderSnapshot, type SwapRecord } from '../../api/swaps';
import { useSwaps } from '../../api/swaps';
import { useWebRgb } from '../../api/webrgb';
import { TakeModal } from '../TakeModal';
import { btnPrimary, btnSecondary, Notice } from '../ui';
import { ladder, marketsOf } from './book';
import { Ladder } from './Ladder';
import { MySwaps } from './MySwaps';
import { PairBar } from './PairBar';
import { Ticket } from './Ticket';

/** The peer-to-peer market: an order book read from the relay, and a take flow that runs through the wallet. */
export function Market({ book }: { book: RgbBookState }) {
  const wallet = useWebRgb();
  const swaps = useSwaps();
  const [network, setNetwork] = useState<string>();
  const [assetId, setAssetId] = useState<string>();
  const [target, setTarget] = useState<{ order: OrderSnapshot; swapId?: string }>();
  const nowSec = Math.floor(Date.now() / 1000);

  const networks = useMemo(() => [...new Set(book.offers.map((o) => o.network))].sort(), [book.offers]);
  const shown = network && networks.includes(network) ? network : wallet.network && networks.includes(wallet.network) ? wallet.network : networks[0];
  const onNetwork = useMemo(() => book.offers.filter((o) => o.network === shown), [book.offers, shown]);
  const markets = useMemo(() => marketsOf(onNetwork), [onNetwork]);
  const market = markets.find((m) => m.assetId === assetId) ?? markets[0];
  const offers = useMemo(() => onNetwork.filter((o) => o.assetId === market?.assetId), [onNetwork, market?.assetId]);
  const { asks, bids } = useMemo(() => ladder(offers), [offers]);
  const ticker = market?.ticker ?? 'asset';
  const lastFill = useMemo(() => {
    const r = swaps.swaps.filter((s) => s.state === 'settled' && s.order.assetId === market?.assetId).sort((a, b) => b.updatedAt - a.updatedAt)[0];
    return r ? r.order.priceSat / (r.order.amount / 10 ** r.order.precision) : undefined;
  }, [swaps.swaps, market?.assetId]);

  const connected = wallet.status === 'connected';
  const take = (o: RgbOffer) => setTarget({ order: snapshotOf(o) });
  const open = (r: SwapRecord) => setTarget({ order: r.order, swapId: r.swapId });

  return (
    <div className="flex flex-col gap-4">
      {book.error && <Notice tone="danger">Relay: {book.error}</Notice>}
      <WalletNotices shown={shown} />

      {markets.length === 0 ? (
        <section className="rounded-2xl bg-surface-card/[0.04] p-10 text-center text-caption text-content-tertiary">
          {book.status === 'connecting' ? 'Connecting to the relay…' : 'No open orders on this network right now.'}
        </section>
      ) : (
        <>
          <PairBar
            markets={markets}
            selected={market}
            onSelect={setAssetId}
            networks={networks}
            network={shown}
            onNetwork={(n) => { setNetwork(n); setAssetId(undefined); }}
            bestBid={bids[0]?.price}
            bestAsk={asks[0]?.price}
            lastFill={lastFill}
            orders={offers.length}
          />
          <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_21rem]">
            <Ladder asks={asks} bids={bids} ticker={ticker} nowSec={nowSec} onPick={take} />
            <div className="flex flex-col gap-4">
              <Ticket ticker={ticker} precision={market?.precision ?? 0} asks={asks} bids={bids} nowSec={nowSec} onTake={take} />
              <MySwaps onOpen={open} />
            </div>
          </div>
        </>
      )}
      {markets.length === 0 && connected && <MySwaps onOpen={open} />}

      {target && <TakeModal key={target.swapId ?? target.order.key} order={target.order} swapId={target.swapId} onClose={() => setTarget(undefined)} />}
    </div>
  );
}

/** Why taking is not possible right now, when it is not: wallet missing, locked, on another network. */
function WalletNotices({ shown }: { shown?: string }) {
  const wallet = useWebRgb();
  const connected = wallet.status === 'connected';
  if (!wallet.canTake) {
    return (
      <Notice tone="info">
        {wallet.status === 'absent'
          ? 'Install the KaleidoSwap browser extension to take orders. It holds your keys and signs the swap; this page never sees them.'
          : 'Your wallet does not support peer-to-peer swaps yet. Update the extension.'}
      </Notice>
    );
  }
  if (!connected) {
    return (
      <Notice tone="info" action={<button type="button" className={clsx(btnPrimary, '!px-3 !py-1.5')} onClick={() => void wallet.connect()}>Connect wallet</button>}>
        Connect your wallet to take an order. The maker must be online to finish the swap.
      </Notice>
    );
  }
  if (wallet.p2pProblem === 'locked') {
    return (
      <Notice tone="warning" action={<button type="button" className={clsx(btnSecondary, '!px-3 !py-1.5')} onClick={() => void wallet.refresh()}>Check again</button>}>
        Your wallet is locked. Click the KaleidoSwap icon in the browser toolbar and unlock it. This page will notice.
      </Notice>
    );
  }
  if (wallet.p2pProblem) {
    return <Notice tone="warning">Your wallet cannot take these orders: {wallet.p2pProblem}. Use an RGB wallet on Mutinynet in the extension.</Notice>;
  }
  if (wallet.network && shown && wallet.network !== shown) {
    return <Notice tone="warning">Your wallet swaps on {wallet.network}; these orders are on {shown}. Switch network in the extension to take them.</Notice>;
  }
  return null;
}
