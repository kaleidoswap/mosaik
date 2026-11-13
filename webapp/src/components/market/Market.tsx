import clsx from 'clsx';
import { useEffect, useMemo, useState } from 'react';
import type { RgbBookState, RgbOffer } from '../../api/rgbp2p';
import { snapshotOf, type OrderSnapshot, type SwapRecord } from '../../api/swaps';
import { useSwaps } from '../../api/swaps';
import { useWebRgb } from '../../api/webrgb';
import { useCatalog } from '../../api/catalog';
import { useRegistry } from '../../api/registry';
import { TakeModal } from '../TakeModal';
import { btnPrimary, btnSecondary, Notice } from '../ui';
import { ladder, marketsOf } from './book';
import { Ladder } from './Ladder';
import { MySwaps } from './MySwaps';
import { PairBar } from './PairBar';
import { Ticket } from './Ticket';
import { assetTrust } from './assetTrust';
import { marketFills, marketStats } from './stats';
import { PriceChart } from './PriceChart';
import { TradeFeed } from './TradeFeed';
import { TrustNotice } from './TrustNotice';
import { useMakerScores } from '../../api/signed';
import { isVerifiedMaker, listingOf, makerMark, marketMakers } from './listing';
import { lastSeen } from '../../api/rgbp2p';
import { MarketMakers } from './Marks';

/** The peer-to-peer market: an order book read from the relay, and a take flow that runs through the wallet. */
export function Market({ book }: { book: RgbBookState }) {
  const wallet = useWebRgb();
  const swaps = useSwaps();
  const [network, setNetwork] = useState<string>();
  const [assetId, setAssetId] = useState<string>();
  const [target, setTarget] = useState<{ order: OrderSnapshot; swapId?: string }>();
  const nowSec = Math.floor(Date.now() / 1000);

  const betaNetwork = import.meta.env.VITE_MARKET_NETWORK ?? 'signetcustom';
  const networks = useMemo(() => [betaNetwork], [betaNetwork]);
  const shown = network && networks.includes(network) ? network : wallet.network && networks.includes(wallet.network) ? wallet.network : networks[0];
  const catalog = useCatalog(shown);
  const onNetwork = useMemo(() => book.offers.filter((o) => o.network === shown && o.expiry > nowSec), [book.offers, shown, nowSec]);
  const eligible = useMemo(() => onNetwork.filter(o => {
    const known = catalog.data?.assets.find(a => a.contractId === o.assetId);
    return !known || (known.precision === o.precision && (!o.ticker || known.ticker === o.ticker));
  }), [onNetwork, catalog.data]);
  const markets = useMemo(() => marketsOf(eligible, catalog.data?.assets), [eligible, catalog.data]);
  useEffect(() => { if (!assetId && markets[0]) setAssetId(markets[0].assetId); }, [assetId, markets]);
  const market = markets.find((m) => m.assetId === assetId) ?? markets[0];
  const offers = useMemo(() => eligible.filter((o) => o.assetId === market?.assetId), [eligible, market?.assetId]);
  const { asks, bids } = useMemo(() => ladder(offers), [offers]);
  const ticker = market?.ticker ?? 'asset';
  const registry = useRegistry(shown);
  const scores = useMakerScores(shown);
  const listingFor = useMemo(() => (id: string) => listingOf(id, catalog.data, registry.data), [catalog.data, registry.data]);
  const listing = market ? listingFor(market.assetId) : undefined;
  const markFor = useMemo(() => (maker: string) => makerMark(maker, market?.assetId ?? '', catalog.data, scores.data), [market?.assetId, catalog.data, scores.data]);
  // a maker is away when every one of its orders here is: one fresh order means it is around
  const awayMakers = useMemo(() => {
    const online = new Set(offers.filter((o) => !o.away).map((o) => o.maker));
    return new Set(offers.filter((o) => o.away && !online.has(o.maker)).map((o) => o.maker));
  }, [offers]);
  const makers = useMemo(() => (market ? marketMakers(market.assetId, offers.map((o) => o.maker), catalog.data, scores.data, undefined, awayMakers) : undefined), [market, offers, catalog.data, scores.data, awayMakers]);
  // live status of the maker under review: from its order while in the book, else its presence alone
  const reviewMaker = useMemo(() => {
    if (!target) return undefined;
    const live = book.offers.find((o) => o.key === target.order.key);
    const presenceAt = book.presence?.get(target.order.maker);
    const seen = live?.lastSeen ?? (presenceAt !== undefined ? lastSeen(0, presenceAt, nowSec) : undefined);
    return { lastSeen: seen || undefined, verified: isVerifiedMaker(scores.data?.get(target.order.maker)) };
  }, [target, book.offers, book.presence, nowSec, scores.data]);
  const trust = useMemo(
    () => (market ? assetTrust({ assetId: market.assetId, ticker: market.ticker }, onNetwork, registry.data) : undefined),
    [market, onNetwork, registry.data],
  );
  const fills = useMemo(() => marketFills(book.fills, market?.assetId, shown), [book.fills, market?.assetId, shown]);
  const stats = useMemo(() => marketStats(fills, nowSec), [fills, nowSec]);
  const lastFill = useMemo(() => {
    const r = swaps.swaps.filter((s) => s.state === 'settled' && s.order.network === shown && s.order.assetId === market?.assetId).sort((a, b) => b.updatedAt - a.updatedAt)[0];
    return r ? r.order.priceSat / (r.order.amount / 10 ** r.order.precision) : undefined;
  }, [swaps.swaps, market?.assetId, shown]);

  const [layout, setLayout] = useState<'book' | 'chart' | 'both'>(() => {
    try { const saved = localStorage.getItem('mosaik.trade.layout'); return saved === 'chart' || saved === 'both' ? saved : 'book'; } catch { return 'book'; }
  });
  const chooseLayout = (value: 'book' | 'chart' | 'both') => { setLayout(value); try { localStorage.setItem('mosaik.trade.layout', value); } catch { /* private mode */ } };
  const connected = wallet.status === 'connected';
  const take = (o: RgbOffer) => setTarget({ order: snapshotOf(o) });
  const open = (r: SwapRecord) => setTarget({ order: r.order, swapId: r.swapId });

  return (
    <div className="flex flex-col gap-4">
      {book.error && <Notice tone="warning">We cannot refresh offers right now. Mosaik will reconnect automatically. Check your connection if this continues.</Notice>}
      {connected && <WalletNotices shown={shown} />}
      {catalog.data?.signedError && <Notice tone="info">The signed curated list could not be checked ({catalog.data.signedError}). Showing the bundled list; no market is marked Kaleido curated until the signature checks out.</Notice>}
      {catalog.isError && <Notice tone="warning">The curated catalog could not be refreshed. {catalog.data ? 'Showing the last loaded list.' : 'Offers can still be discovered; curated assets will return when the catalog is available.'}</Notice>}
      {eligible.length !== onNetwork.length && <Notice tone="warning">Some offers were hidden because their metadata conflicts with the curated asset identity.</Notice>}
      {registry.isError && <Notice tone="warning">RGBMap is unavailable. {registry.data ? 'Showing cached registry information; it may be out of date.' : 'Listing status is unknown, not unlisted.'}</Notice>}

      {markets.length === 0 ? (
        <section className="empty-market" aria-live="polite">
          <span className="eyebrow">SIGNET ORDER BOOK</span>
          <h2>{book.status === 'connecting' ? 'Finding the market…' : 'The next trade starts with an offer.'}</h2>
          <p>{book.status === 'connecting' ? 'Connecting to the relay to discover available orders.' : 'There are no active offers on this network right now. New offers appear here automatically. Makers can publish an offer from the desktop app.'}</p>
          <a href="#learn">Learn how to make or take an offer ↗</a>
        </section>
      ) : (
        <>
          <div id="market-offers" tabIndex={-1}><PairBar
            markets={markets}
            selected={market}
            onSelect={setAssetId}
            networks={networks}
            network={shown}
            onNetwork={(n) => { setNetwork(n); setAssetId(undefined); }}
            bestBid={bids[0]?.price}
            bestAsk={asks[0]?.price}
            lastFill={lastFill ?? stats.last}
            stats={stats}
            orders={offers.length}
            listingFor={listingFor}
          /></div>
          {trust && market && (trust.level === 'warning' || trust.level === 'caution' || listing?.tier === 'unlisted') && <TrustNotice trust={trust} assetId={market.assetId} listing={listing} />}
          <div className="trading-workspace">
            <div className="market-visuals">
              <div className="market-view-toolbar"><div role="group" aria-label="Market view" className="view-switch">{(['book', 'chart', 'both'] as const).map(value => <button key={value} aria-pressed={layout === value} onClick={() => chooseLayout(value)}>{value === 'book' ? 'Order book' : value === 'chart' ? 'Chart' : 'Both'}</button>)}</div><span className="book-live"><i className={book.status === 'open' ? 'online' : ''} />{book.status === 'open' ? 'Live orders' : 'Reconnecting'}</span></div>
              {layout !== 'chart' && <Ladder asks={asks} bids={bids} ticker={ticker} nowSec={nowSec} onPick={take} markFor={markFor} />}
              {layout !== 'book' && <PriceChart points={stats.points} ticker={ticker} />}
              <TradeFeed fills={fills} ticker={ticker} nowSec={nowSec} />
            </div>
            <div className="market-ticket"><Ticket ticker={ticker} precision={market?.precision ?? 0} asks={asks} bids={bids} nowSec={nowSec} onTake={take} /><MySwaps onOpen={open} /></div>
          </div>
          {makers && <MarketMakers rows={makers.rows} sat={makers.sat} trades={makers.trades} ticker={ticker} scoresLoaded={!!scores.data} />}
          {trust && market && trust.level !== 'warning' && trust.level !== 'caution' && listing?.tier !== 'unlisted' && <TrustNotice trust={trust} assetId={market.assetId} listing={listing} />}
          <div className="maker-inline"><span><strong>Want to list an asset?</strong> Publish an offer from the desktop app or run an automated p2pd maker. Keep it online to fill trades.</span><a href="#learn">Maker guide ↗</a></div>
        </>
      )}
      {markets.length === 0 && connected && <MySwaps onOpen={open} />}

      {target && <TakeModal key={target.swapId ?? target.order.key} order={target.order} swapId={target.swapId} maker={reviewMaker} onClose={() => setTarget(undefined)} />}
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
