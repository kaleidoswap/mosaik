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
  const step = !networkReady ? 0 : !funded ? 1 : !prepared ? 2 : 3;
  const titles = ['Connect your wallet', 'Add test BTC', 'Prepare your wallet', 'Choose an offer'];
  const descriptions = [
    'Use the KaleidoSwap browser extension. Your keys and approvals stay there.',
    'Copy your RGB receive address from the extension, then request coins from the Mutinynet faucet.',
    'Set aside two 10,000-sat outputs for RGB assets, plus network fees. Review the transaction in your extension.',
    'Select an asset below, choose Buy or Sell, and review the full cost before approving.',
  ];
  if (step === 3) return <div className="ready-strip"><span><i aria-hidden="true">✓</i> Wallet ready <small>Choose an offer to trade.</small></span><a href="#learn">Setup & help ↗</a></div>;
  return <Card className="onboarding-card journey-card compact-journey"><CardContent>
    <div className="journey-heading"><span className="eyebrow">WALLET SETUP</span><button className="text-link" disabled={!connected || busy} onClick={() => void syncWallet()}>{busy ? 'Syncing…' : 'Refresh wallet'}</button></div>
    <ol className="journey-track" aria-label="Wallet setup progress">{['Connect', 'Fund', 'Prepare', 'Trade'].map((label, i) => <li key={label} aria-current={step === i ? 'step' : undefined} className={i < step ? 'is-done' : i === step ? 'is-current' : ''}><span>{i < step ? '✓' : i + 1}</span>{label}</li>)}</ol>
    <div className="journey-action"><div><h2>{titles[step]}</h2><p>{descriptions[step]}</p></div><div className="journey-cta">
      {step === 0 && (connected ? <><span className="setup-status">{wallet.p2pProblem === 'locked' ? 'Unlock the extension to continue' : 'Select RGB · Mutinynet in the extension'}</span><Button variant="secondary" disabled={busy} onClick={() => void syncWallet()}>Check connection</Button></> : wallet.status === 'absent' ? <a className="btn-primary" href="#learn">Set up the extension ↗</a> : <Button variant="cta" disabled={wallet.status === 'connecting'} onClick={() => void wallet.connect()}>Connect wallet</Button>)}
      {step === 1 && <a className="btn-primary" href="https://faucet.mutinynet.com" target="_blank" rel="noopener noreferrer">Get test BTC ↗</a>}
      {step === 2 && (wallet.provider?.createUtxos ? <Button variant="cta" disabled={busy || (wallet.balance?.spendable ?? 0) < 22000} onClick={() => void prepare()}>{busy ? 'Waiting for wallet…' : 'Prepare wallet'}</Button> : <a className="btn-primary" href="#wallet">Open wallet instructions ↗</a>)}
    </div></div>
    {step === 1 && <p className="helper-copy">After confirmation, select Refresh wallet. Use test coins only.</p>}
    {step === 2 && (wallet.balance?.spendable ?? 0) < 22000 && <p className="helper-copy">You need at least 22,000 spendable sat for this preparation. <a href="https://faucet.mutinynet.com" target="_blank" rel="noopener noreferrer">Get more test BTC ↗</a></p>}
    <button className="text-link journey-help" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? 'Hide details −' : 'How do the apps work together? +'}</button>
    {expanded && <div className="journey-explainer"><p><strong>Mosaik</strong> finds offers. <strong>The extension</strong> holds your trading wallet and approves swaps. <strong>Desktop</strong> lets you publish your own offers and must stay online to complete them.</p><a href="#learn">See the complete guide ↗</a></div>}
    {message && <p role="status" className="helper-copy">{message}</p>}
  </CardContent></Card>;
}
