import { lazy, Suspense, useEffect, useState } from 'react';
import { Button, Card, CardContent } from 'kaleido-ui';
import kaleidoMark from 'kaleido-ui/brand/kaleidoswap-pictogram.svg?url';
import { useRgbP2pBook } from './api/rgbp2p';
import { WebRgbProvider } from './api/webrgb';
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
          {view === 'trade' && <><div className="page-heading"><div><span className="eyebrow">RGB × BITCOIN</span><h1>Trade on your terms.</h1><p>Choose an asset. Review an offer. Approve in your wallet.</p></div><a className="text-link" href="#learn">How it works ↗</a></div><div className="preview-notice"><span aria-hidden="true">◈</span><p><strong>Test coins only.</strong> Mutinynet · TESTUSD and TESTXAU have no real-world value.</p></div><Market book={book} /></>}
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
  return <a href="#trade" className="mosaik-brand" aria-label="Mosaik home"><span className="mosaik-symbol" aria-hidden="true"><i /><i /><i /><i /></span><span>Mosaik<small>BY KALEIDOSWAP</small></span></a>;
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
    <Card className="desktop-bridge"><CardContent><span className="eyebrow">WANT TO SET YOUR OWN PRICE?</span><h2>Publish on desktop. Discover on Mosaik.</h2><ol className="bridge-steps"><li><strong>1 · Open desktop</strong><span>Choose P2P trading, then Mutinynet. No Lightning node is needed.</span></li><li><strong>2 · Fund its wallet</strong><span>The desktop wallet is separate from the extension. Use its own receive address.</span></li><li><strong>3 · Create a limit offer</strong><span>Set the asset, quantity and price. Keep desktop running until the trade finishes.</span></li></ol><p className="helper-copy">Already have the private desktop build? Open it from your Applications folder. Installers are shared separately with testers; there is no public beta download.</p><Button variant="cta" onClick={() => { window.location.hash = 'trade'; }}>Find offers on Mosaik</Button></CardContent></Card>
    <details className="learn-more"><summary>Trading details & app roles</summary><div className="learn-details"><Card><CardContent><span className="eyebrow">TWO SIDES, ONE TRADE</span><h2>Take an offer here. Make one on desktop.</h2><p>The browser market lets you accept an existing offer. To create an offer, use the desktop app or p2pd, fund your Signet wallet and keep the maker running until your trade finishes.</p><Button variant="cta" onClick={() => { window.location.hash = 'trade'; }}>Explore the market</Button></CardContent></Card><Card><CardContent><span className="eyebrow">BEFORE YOUR FIRST TRADE</span><h2>Know what you are trading.</h2><ul className="explain-list"><li>Tickers are names, not identities. Check the asset ID and verification label.</li><li>Keep enough test BTC for fees and the RGB output. The review shows the full cost.</li><li>Prices and activity reported by makers are not independently verified market statistics.</li><li>Some trades need an unused RGB-ready output (UTXO). Prepare one in the extension; this uses test BTC and network fees.</li></ul></CardContent></Card></div>
    <Card className="product-note"><CardContent><span className="eyebrow">ONE FAMILY, DIFFERENT JOBS</span><h2>Mosaik trades. KaleidoSwap routes.</h2><p>Mosaik is an order-book market where you choose another trader’s offer. The KaleidoSwap web app guides you through quoted swap routes. Covenant research lives separately in Labs, with different networks and settlement rules.</p></CardContent></Card></details>
  </>;
}
function Labs() {
  const [open, setOpen] = useState(false);
  return <><PageHeading eyebrow="KALEIDOSWAP LABS" title="Ideas for the next market." text="Research belongs in its own space. These experiments are separate from the Signet RGB market and are not part of this trading beta." />
    <Card className="lab-card"><CardContent><div className="lab-symbol" aria-hidden="true">◇</div><span className="eyebrow">RESEARCH · LIQUID / SIMPLICITY</span><h2>Covenant DEX / Tessera</h2><p>An offer whose settlement rules are enforced by a covenant. This explores a different model from the interactive maker-and-wallet flow used by Mosaik’s RGB market.</p><div className="lab-state">Roadmap to be defined</div><p className="helper-copy">Separate wallet requirements, backend and network. No Covenant trading is enabled in this preview.</p>{LAB_ENABLED && <Button variant="secondary" onClick={() => setOpen(!open)}>{open ? 'Close research workspace' : 'Open research workspace'}</Button>}</CardContent></Card>
    {LAB_ENABLED && open && <Suspense fallback={<p>Loading research workspace…</p>}><CovenantWorkspace /></Suspense>}
  </>;
}
