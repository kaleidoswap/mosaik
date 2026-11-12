import { lazy, Suspense, useEffect, useState } from 'react';
import { Button, Card, CardContent } from 'kaleido-ui';
import kaleidoMark from 'kaleido-ui/brand/kaleidoswap-pictogram.svg?url';
import { useRgbP2pBook } from './api/rgbp2p';
import { WebRgbProvider, useWebRgb } from './api/webrgb';
import { SwapsProvider, type SwapRecord } from './api/swaps';
import { Market } from './components/market/Market';
import { MySwaps } from './components/market/MySwaps';
import { TakeModal } from './components/TakeModal';
import { WalletButton } from './components/WalletButton';
import { WalletPanel } from './components/WalletPanel';

const CovenantWorkspace = lazy(() => import('./components/CovenantWorkspace'));
const LAB_ENABLED = import.meta.env.VITE_ENABLE_COVENANT_LAB === 'true';
const NAV = [
  { id: 'trade', label: 'Trade', glyph: '↗' },
  { id: 'activity', label: 'Activity', glyph: '≋' },
  { id: 'wallet', label: 'Wallet', glyph: '◫' },
  { id: 'learn', label: 'How it works', glyph: '?' },
  { id: 'labs', label: 'Labs', glyph: '◇' },
] as const;
type View = typeof NAV[number]['id'];
function readView(): View {
  const hash = window.location.hash.slice(1);
  return NAV.some(n => n.id === hash) ? hash as View : 'trade';
}
export function App() {
  const [welcome, setWelcome] = useState(() => !window.location.hash || window.location.hash === '#welcome');
  useEffect(() => {
    const update = () => setWelcome(!window.location.hash || window.location.hash === '#welcome');
    window.addEventListener('hashchange', update);
    return () => window.removeEventListener('hashchange', update);
  }, []);
  if (welcome) return <Welcome />;
  return <WebRgbProvider><SwapsProvider><Shell /></SwapsProvider></WebRgbProvider>;
}
function Shell() {
  const [view, setView] = useState<View>(readView);
  const book = useRgbP2pBook();
  useEffect(() => {
    const update = () => setView(readView());
    window.addEventListener('hashchange', update);
    return () => window.removeEventListener('hashchange', update);
  }, []);
  const live = book.status === 'open';
  return (
    <div className="mosaik-shell">
      <a href="#main" className="skip-link" onClick={event => { event.preventDefault(); document.getElementById("main")?.focus(); }}>Skip to content</a>
      <aside className="mosaik-sidebar">
        <Brand />
        <div className="sidebar-caption">YOUR MARKETPLACE</div>
        <nav aria-label="Main navigation" className="mosaik-nav">
          {NAV.map(n => <a key={n.id} href={`#${n.id}`} aria-current={view === n.id ? 'page' : undefined} className={view === n.id ? 'nav-item is-active' : 'nav-item'}><span aria-hidden="true">{n.glyph}</span>{n.label}{n.id === 'labs' && <small>R&D</small>}</a>)}
        </nav>
        <div className="sidebar-bottom">
          <div className="maker-note"><span className="eyebrow">MAKE THE MARKET</span><h3>Have an offer?</h3><p>Set your price in the desktop app. Keep it online to fill your offers.</p><a href="#learn">Desktop trading guide <span aria-hidden="true">↗</span></a></div>
          <div className="family-signature"><img src={kaleidoMark} alt="" />Part of KaleidoSwap</div>
        </div>
      </aside>
      <div className="mosaik-main">
        <header className="mosaik-topbar">
          <div className="mobile-brand"><Brand /></div>
          <div className="network-label"><span className="network-dot" />Signet <span className="preview-label">PRIVATE PREVIEW</span></div>
          <div className="topbar-actions"><span className={`connection-label ${live ? 'is-live' : ''}`} title={book.error ?? 'Connection to the order discovery relay'}><span />{live ? 'Relay connected' : book.status === 'connecting' ? 'Connecting…' : 'Reconnecting…'}</span><WalletButton /></div>
        </header>
        <nav className="mobile-nav" aria-label="Mobile navigation">{NAV.map(n => <a key={n.id} href={`#${n.id}`} aria-current={view === n.id ? 'page' : undefined}>{n.label}</a>)}</nav>
        <main id="main" tabIndex={-1} className="mosaik-content">
          {view === 'trade' && <><div className="trading-heading"><h1>Market</h1><a className="text-link" href="#learn">Trading guide ↗</a></div><nav className="swap-mode-nav" aria-label="Swap model"><a href="#trade" aria-current="page">Interactive <span>RGB · maker online</span></a><a href="#labs">Non-interactive <span>Liquid & Arkade · Labs ↗</span></a></nav><TradingEntry><Market book={book} /></TradingEntry></>}
          {view === 'activity' && <><PageHeading eyebrow="YOUR TRADING HISTORY" title="Follow every trade." text="See what is waiting for approval, in progress or completed. Your wallet continues the swap if you close this page." /><Activity /></>}
          {view === 'wallet' && <><PageHeading eyebrow="SELF-CUSTODY" title="Your wallet, connected." text="Mosaik reads the balances you share. Keys, approvals and signing stay in your browser extension." /><WalletPanel /></>}
          {view === 'learn' && <Learn />}
          {view === 'labs' && <Labs />}
          <footer className="mosaik-footer"><span>Mosaik · Peer-to-peer RGB trading</span><div><a href="#learn">How it works</a><a href="#labs">Research & Labs</a><span>Signet only</span></div></footer>
        </main>
      </div>
    </div>
  );
}
function Brand() {
  return <a href="#welcome" className="mosaik-brand" aria-label="Mosaik home"><span className="mosaik-symbol" aria-hidden="true"><i /><i /><i /><i /></span><span>Mosaik<small>BY KALEIDOSWAP</small></span></a>;
}
function PageHeading({ eyebrow, title, text }: { eyebrow: string; title: string; text: string }) {
  return <div className="page-heading"><div><span className="eyebrow">{eyebrow}</span><h1>{title}</h1><p>{text}</p></div></div>;
}
function Activity() {
  const [record, setRecord] = useState<SwapRecord>();
  return <><MySwaps onOpen={setRecord} />{record && <TakeModal order={record.order} swapId={record.swapId} onClose={() => setRecord(undefined)} />}<p className="helper-copy">This browser remembers the trades started here. Reconnect the same wallet to refresh their status.</p></>;
}
function Learn() {
  return <>
    <PageHeading eyebrow="A CLEAR PATH FROM OFFER TO OWNERSHIP" title="Trade directly. Stay in control." text="Mosaik brings offers together. The KaleidoSwap extension handles your wallet and the trade, while the maker runs the other side." />
    <div className="learn-grid">{[
      ['01', 'Connect your wallet', 'Use desktop Chrome with the private KaleidoSwap extension build supplied to testers. Unlock it, choose RGB → Mutinynet, then return here and connect.'],
      ['02', 'Choose an offer', 'Compare the available buyers and sellers. Each offer has a fixed quantity and an expiry. You take the whole offer, not part of it.'],
      ['03', 'Review, then approve', 'Check the asset ID, quantity, fees and final total. Approval happens in your extension; connecting a wallet does not approve a trade.'],
      ['04', 'Follow the result', 'Watch the trade in Activity. The maker must stay online. If something interrupts the flow, reopen the trade and follow the recovery instructions.'],
    ].map(([step,title,text]) => <Card key={step} className="guide-card"><CardContent><span className="step-number">{step}</span><h2>{title}</h2><p>{text}</p></CardContent></Card>)}</div>
    <Card className="desktop-bridge"><CardContent><span className="eyebrow">WANT TO SET YOUR OWN PRICE?</span><h2>Publish on desktop. Discover on Mosaik.</h2><ol className="bridge-steps"><li><strong>1 · Open desktop</strong><span>Choose P2P trading, then Mutinynet. No Lightning node is needed.</span></li><li><strong>2 · Fund its wallet</strong><span>The desktop wallet is separate from the extension. Use its own receive address.</span></li><li><strong>3 · Create a limit offer</strong><span>Set the asset, quantity and price. Keep desktop running until the trade finishes.</span></li></ol><p className="helper-copy">Prefer automation? Run p2pd with the team’s Signet maker configuration and funded wallet; it maintains offers while the service stays online. Publishing an offer does not verify an asset or add it to the curated registry. Already have the private desktop build? Open it from your Applications folder. Installers are shared separately with testers; there is no public beta download.</p><Button variant="cta" onClick={() => { window.location.hash = 'trade'; }}>Find offers on Mosaik</Button></CardContent></Card>
    <details className="learn-more"><summary>Trading details & app roles</summary><div className="learn-details"><Card><CardContent><span className="eyebrow">TWO SIDES, ONE TRADE</span><h2>Take an offer here. Make one on desktop.</h2><p>The browser market lets you accept an existing offer. To create an offer, use the desktop app or p2pd, fund your Signet wallet and keep the maker running until your trade finishes.</p><Button variant="cta" onClick={() => { window.location.hash = 'trade'; }}>Explore the market</Button></CardContent></Card><Card><CardContent><span className="eyebrow">BEFORE YOUR FIRST TRADE</span><h2>Know what you are trading.</h2><ul className="explain-list"><li>Tickers are names, not identities. Check the asset ID and verification label.</li><li>Keep enough test BTC for fees and the RGB output. The review shows the full cost.</li><li>Prices and activity reported by makers are not independently verified market statistics.</li><li>Some trades need an unused RGB-ready output (UTXO). Prepare one in the extension; this uses test BTC and network fees.</li></ul></CardContent></Card></div>
    <Card className="product-note"><CardContent><span className="eyebrow">ONE FAMILY, DIFFERENT JOBS</span><h2>Mosaik trades. KaleidoSwap routes.</h2><p>Mosaik is an order-book market where you choose another trader’s offer. The KaleidoSwap web app guides you through quoted swap routes. Covenant research lives separately in Labs, with different networks and settlement rules.</p></CardContent></Card></details>
  </>;
}
function Labs() {
  const [backend, setBackend] = useState<'liquid' | 'banco'>('liquid');
  const [open, setOpen] = useState(false);
  return <>
    <PageHeading eyebrow="KALEIDOSWAP LABS · NON-INTERACTIVE SWAPS" title="Fund an offer. Let the market come to you." text="Explore DEX offers whose spending rules are set in advance. Once an offer is funded and published, the maker does not need to come back online to sign a fill." />
    <nav className="swap-mode-nav" aria-label="Swap model"><a href="#trade">Interactive <span>RGB · maker online ↗</span></a><a href="#labs" aria-current="page">Non-interactive <span>Liquid & Arkade · Labs</span></a></nav>
    <div className="labs-flow" aria-label="Non-interactive swap flow"><div><span>01</span><strong>Maker funds</strong><p>Lock the offered assets with the price and spend conditions.</p></div><div><span>02</span><strong>Offer stays available</strong><p>The maker can disconnect after funding and publication are complete.</p></div><div><span>03</span><strong>Taker fulfils</strong><p>Pay the required asset. The spending rules release the offered asset.</p></div></div>
    <div className="labs-dex-grid">
      <Card className="labs-dex-card"><CardContent><span className="eyebrow">LIQUID ASSETS · SIMPLICITY</span><h2>Covenant DEX / Tessera</h2><p>Exchange Liquid assets through a funded covenant UTXO. Settlement enforces the maker’s payment without a fresh maker signature.</p><div className="labs-facts"><span>Assets<strong>L-BTC & Liquid-issued assets</strong></span><span>Maker presence<strong>Offline after funding & publication</strong></span><span>Availability<strong>Integration pending · not enabled here</strong></span></div><details><summary>How settlement works</summary><p>The taker spends the offer under its covenant conditions. Liquid chain access and a compatible wallet are still required. An offer must remain unspent and valid; reclaim conditions are defined by the contract.</p></details>{LAB_ENABLED && <Button variant="secondary" onClick={() => { setBackend('liquid'); setOpen(true); }}>Open Liquid research workspace</Button>}</CardContent></Card>
      <Card className="labs-dex-card"><CardContent><span className="eyebrow">ARKADE ASSETS · BANCO</span><h2>Arkade DEX / Banco</h2><p>Exchange Arkade assets through a funded covenant VTXO. The taker fulfils the published terms without asking the maker to sign again.</p><div className="labs-facts"><span>Assets<strong>BTC on Arkade & Arkade assets</strong></span><span>Maker presence<strong>Offline after funding & publication</strong></span><span>Availability<strong>Integration pending · not enabled here</strong></span></div><details><summary>How settlement works</summary><p>Ark services and the introspector must remain available. Maker-offline does not mean infrastructure-free. VTXO expiry, cancellation and exit conditions still apply; these must be verified before funding an offer.</p></details>{LAB_ENABLED && <Button variant="secondary" onClick={() => { setBackend('banco'); setOpen(true); }}>Open Arkade research workspace</Button>}</CardContent></Card>
    </div>
    <div className="labs-scope"><h2>One app. Two ways to trade.</h2><p><strong>Interactive RGB swaps</strong> are available in the private Signet market. Maker and taker exchange messages and sign the transaction together.</p><p><strong>Non-interactive DEX offers</strong> are the Labs track. The maker commits funds and terms ahead of time; no new maker signature is needed to fill a valid offer.</p><p>Liquid and Arkade are separate markets, with separate assets and wallet requirements. This does not enable Liquid ↔ Arkade cross-chain swaps or an automatic refund for the RGB market.</p><a className="text-link" href="#trade">Open the interactive RGB market ↗</a></div>
    <details className="learn-more"><summary>What remains before Labs trading opens</summary><div className="labs-readiness"><p>Deploy the dedicated backends, verify wallet and asset support, then complete funded maker-offline, cancellation, expiry and recovery tests on each network.</p><p>The existing Banco integration also needs a pinned, maintained SDK path: its original upstream repository is archived. Cross-chain settlement remains a separate research track with unresolved protocol primitives.</p><p>No Liquid or Arkade trading is enabled in this preview. Connecting the RGB wallet does not authorize spending on either network.</p></div></details>
    {LAB_ENABLED && open && <><Button variant="secondary" onClick={() => setOpen(false)}>Close research workspace</Button><Suspense fallback={<p>Loading research workspace…</p>}><CovenantWorkspace key={backend} initialBackend={backend} /></Suspense></>}
  </>;
}

function Welcome() {
  return <div className="welcome-page">
    <header className="welcome-header"><Brand /><span className="welcome-badge"><span className="network-dot" /> Internal preview · Signet</span></header>
    <main id="welcome-main" className="welcome-main">
      <section className="welcome-hero" aria-labelledby="welcome-title">
        <div className="welcome-copy"><span className="eyebrow">THE PEER-TO-PEER RGB MARKET</span><h1 id="welcome-title">Your wallet.<br />Your next <em>trade.</em></h1><p>Discover RGB assets. Trade directly with another participant, using your KaleidoSwap wallet.</p><Button variant="cta" onClick={() => { window.location.hash = 'trade'; }}>Enter Mosaik <span aria-hidden="true">↗</span></Button><span className="welcome-caption">Connect your wallet inside. Every trade needs your approval.</span></div>
        <div className="welcome-art" aria-hidden="true"><div className="welcome-orbit" /><div className="welcome-tiles"><i /><i /><i /><i /></div><span className="asset-chip asset-usd">TESTUSD <small>Test dollar</small></span><span className="asset-chip asset-xau">TESTXAU <small>Test gold</small></span><span className="art-caption">RGB ASSETS · BITCOIN SETTLEMENT</span></div>
      </section>
      <section className="welcome-path" aria-label="Getting started">{[
        ['01', 'Connect', 'Unlock KaleidoSwap in desktop Chrome and connect your wallet.'],
        ['02', 'Get ready', 'Choose Mutinynet and fund your wallet with test coins. We’ll guide you.'],
        ['03', 'Trade', 'Choose an offer, review the total and approve in your extension.'],
      ].map(([number, title, copy]) => <div key={number}><span>{number}</span><h2>{title}</h2><p>{copy}</p></div>)}</section>
      <div className="welcome-test-note"><span aria-hidden="true">◈</span><p><strong>A place to test, with test coins.</strong> TESTUSD and TESTXAU have no real-world value. This internal preview runs on Mutinynet, a custom Signet.</p><a href="#learn">How trading works ↗</a></div>
    </main><footer className="welcome-footer"><span>Built with KaleidoSwap</span><span>Your keys stay in your wallet.</span></footer>
  </div>;
}
function TradingEntry({ children }: { children: React.ReactNode }) {
  const wallet = useWebRgb();
  if (wallet.status === 'connected') return <>{children}</>;
  return <Card className="wallet-entry"><CardContent><div className="entry-icon" aria-hidden="true">◫</div><span className="eyebrow">YOUR FIRST STEP</span><h2>Connect KaleidoSwap.</h2><p>Your wallet is your way in. Connect to see your balances and start trading.</p>
    {wallet.status === 'absent' ? <><a className="btn-primary" href="#learn">Set up KaleidoSwap ↗</a><p className="helper-copy">Use desktop Chrome with the private extension supplied to testers. Already installed? Unlock it and reload this page.</p></> : <><Button variant="cta" disabled={wallet.status === 'connecting'} onClick={() => void wallet.connect()}>{wallet.status === 'connecting' ? 'Approve in KaleidoSwap…' : 'Connect KaleidoSwap'}</Button><p className="helper-copy">Unlock the extension, then approve this connection. No email or password is entered on Mosaik.</p></>}
    {wallet.error && <p role="alert" className="entry-error">{wallet.error}</p>}<div className="entry-trust">Connecting does not authorize a trade.<br />You review and approve each swap in your wallet.</div><a className="text-link" href="#learn">New here? Follow the setup guide ↗</a>
  </CardContent></Card>;
}
