import { useState } from 'react';
import { Button, Card, CardContent } from 'kaleido-ui';
import { useWebRgb } from '../../api/webrgb';

export function Onboarding() {
  const wallet = useWebRgb();
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [message, setMessage] = useState('');
  const connected = wallet.status === 'connected';
  const networkReady = connected && wallet.network === 'signetcustom' && !wallet.p2pProblem;
  const funded = networkReady && (wallet.balance?.spendable ?? 0) > 0;
  const prepared = networkReady && (wallet.balance?.freeColorableUtxos ?? 0) > 0;
  async function syncWallet() {
    if (!connected || busy) return;
    setBusy(true); setMessage('Syncing your wallet with the network…');
    try {
      await wallet.provider?.refresh?.();
      await wallet.refresh();
      setMessage('Wallet synced. Only confirmed, spendable BTC is available for trading.');
    } catch (e) { setMessage(e instanceof Error ? e.message : 'Wallet sync failed. Try again.'); }
    finally { setBusy(false); }
  }
  async function prepare() {
    if (!networkReady || !wallet.provider?.createUtxos) return;
    setBusy(true); setMessage('Review the preparation transaction in your extension.');
    try {
      await wallet.provider.createUtxos({ num: 2, size: 10000, feeRate: 2 });
      setMessage('Preparation submitted. Wait for confirmation, then refresh your wallet.');
      await wallet.refresh();
    } catch (e) { setMessage(e instanceof Error ? e.message : 'Wallet preparation failed. Try again in your extension.'); }
    finally { setBusy(false); }
  }
  return <Card className="onboarding-card"><CardContent>
    <div className="onboarding-heading"><div><span className="eyebrow">YOUR FIRST SIGNET TRADE</span><h2>{prepared ? 'Your wallet is ready to trade.' : 'Get ready in three steps.'}</h2></div><Button variant="secondary" onClick={() => void syncWallet()} disabled={!connected || busy}>{busy ? 'Syncing…' : 'Refresh wallet'}</Button></div>
    {prepared && <button type="button" className="text-link" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? 'Hide setup steps' : 'Show setup steps'}</button>}
    {(!prepared || expanded) && <div className="onboarding-steps">
      <div className={networkReady ? 'setup-step is-done' : 'setup-step'}><span className="setup-number">{networkReady ? '✓' : '01'}</span><h3>Connect on Mutinynet</h3><p>Unlock the KaleidoSwap extension and select your RGB wallet on Mutinynet.</p>{connected ? <span className="setup-status">{networkReady ? 'Connected on the right network' : wallet.p2pProblem === 'locked' ? 'Unlock your extension' : 'Switch to Mutinynet in the extension'}</span> : <Button variant="cta" disabled={wallet.status === 'absent' || wallet.status === 'connecting'} onClick={() => void wallet.connect()}>{wallet.status === 'absent' ? 'Extension required' : 'Connect wallet'}</Button>}</div>
      <div className={funded ? 'setup-step is-done' : 'setup-step'}><span className="setup-number">{funded ? '✓' : '02'}</span><h3>Add test BTC</h3><p>Copy your RGB wallet’s BTC receive address in the extension. Fund it using the Mutinynet faucet, then wait for confirmation.</p><a href="https://faucet.mutinynet.com" target="_blank" rel="noopener noreferrer">Open test BTC faucet ↗</a>{funded && <span className="setup-status">{wallet.balance?.spendable.toLocaleString()} confirmed sat available</span>}</div>
      <div className={prepared ? 'setup-step is-done' : 'setup-step'}><span className="setup-number">{prepared ? '✓' : '03'}</span><h3>Prepare for RGB</h3><p>Reserve two 10,000-sat outputs in your own wallet, plus network fees. Your extension asks for approval. Each offer shows its full trading cost.</p>{prepared ? <span className="setup-status">RGB outputs available · choose an offer below</span> : wallet.provider?.createUtxos ? <Button variant="secondary" disabled={!funded || busy || (wallet.balance?.spendable ?? 0) < 22000} onClick={() => void prepare()}>{busy ? 'Waiting for wallet…' : 'Prepare wallet'}</Button> : <a href="#wallet">Prepare outputs in your extension ↗</a>}</div>
    </div>}
    {message && <p role="status" className="helper-copy">{message}</p>}
  </CardContent></Card>;
}
