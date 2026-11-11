import clsx from 'clsx';
import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { failure, failureFromError, preflight, type Failure } from '../api/swapProblems';
import { isTerminal, MAKER_WAIT_SECONDS, useSwaps, type OrderSnapshot, type SwapRecord } from '../api/swaps';
import { useWebRgb } from '../api/webrgb';
import { shortId } from '../lib/format';
import { EXPLORER_TX, fmtClock, fmtLeft, fmtPrice, fmtUnits } from '../lib/units';
import { btnPrimary, btnSecondary, CopyValue, Icon, Notice, Pill, Spinner } from './ui';

type View = 'review' | 'approve' | 'tracking' | 'problem';

const STEPS = ['Review', 'Approve', 'Maker', 'Sign', 'Broadcast', 'Done'] as const;
const FOCUSABLE = 'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

const sat = (n: number) => `${n.toLocaleString()} sat`;

/**
 * Takes one order, start to finish. The page never signs: the extension shows its own approval window
 * and runs the swap, and this dialog only shows what the wallet reports. Closing it never stops a swap
 * that is already running; it keeps going in the wallet and stays listed under "My swaps".
 */
export function TakeModal({ order, swapId: existing, onClose }: { order: OrderSnapshot; swapId?: string; onClose: () => void }) {
  const wallet = useWebRgb();
  const swaps = useSwaps();
  const [view, setView] = useState<View>(existing ? 'tracking' : 'review');
  const [swapId, setSwapId] = useState(existing);
  const [problem, setProblem] = useState<Failure>();
  const [now, setNow] = useState(() => Date.now());
  const [hint, setHint] = useState('');
  const [connecting, setConnecting] = useState(false);
  const attempt = useRef(0);
  const dialog = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const liveId = useId();

  const record: SwapRecord | undefined = swapId ? swaps.swaps.find((r) => r.swapId === swapId) : undefined;
  const state = record?.state;
  const nowSec = Math.floor(now / 1000);

  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(t);
  }, []);

  const checks = useMemo(
    () =>
      preflight(
        order,
        { connected: wallet.status === 'connected', canTake: wallet.canTake, p2pProblem: wallet.p2pProblem, network: wallet.network, balance: wallet.balance, assets: wallet.assets },
        nowSec,
      ),
    [order, wallet.status, wallet.canTake, wallet.p2pProblem, wallet.network, wallet.balance, wallet.assets, nowSec],
  );
  const blockers = checks.filter((c) => c.level === 'block');

  const finished = view === 'tracking' && !!state && isTerminal(state);
  const running = view === 'tracking' && !finished;

  const cancelApproval = useCallback(() => {
    attempt.current += 1; // a late answer from the wallet no longer drives this dialog
    setView('review');
  }, []);

  const requestClose = useCallback(() => {
    if (view === 'approve') {
      cancelApproval();
      return;
    }
    onClose();
  }, [view, cancelApproval, onClose]);

  // Esc closes the dialog unless that would hide a decision in progress.
  const onKey = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        if (running) setHint('The swap is running in your wallet. Use “Close window” to hide this; it keeps going.');
        else requestClose();
        return;
      }
      if (e.key !== 'Tab' || !dialog.current) return;
      const items = [...dialog.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      if (!items.length) {
        e.preventDefault();
        dialog.current.focus();
        return;
      }
      const first = items[0]!;
      const last = items[items.length - 1]!;
      const active = document.activeElement;
      if (e.shiftKey && (active === first || active === dialog.current)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    },
    [running, requestClose],
  );
  useEffect(() => {
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onKey]);

  // Focus goes in on open and back to where it was on close; the page behind does not scroll.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    dialog.current?.focus();
    return () => {
      document.body.style.overflow = overflow;
      before?.focus?.();
    };
  }, []);

  // Each step gets focus on its heading so screen readers and keyboards follow along.
  const stage = view === 'tracking' ? (state ?? 'awaiting_maker') : view;
  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>('[data-autofocus]')?.focus();
  }, [stage]);

  const proceed = async () => {
    if (blockers.length) return;
    if (wallet.status !== 'connected') {
      setConnecting(true);
      const ok = await wallet.connect();
      setConnecting(false);
      if (!ok) return;
    }
    const my = ++attempt.current;
    setProblem(undefined);
    setView('approve');
    try {
      const rec = await swaps.start(order);
      if (attempt.current === my) {
        setSwapId(rec.swapId);
        setView('tracking');
      }
    } catch (e) {
      if (attempt.current === my) {
        setProblem(failureFromError(e));
        setView('problem');
      }
    }
  };

  const o = order;
  const ticker = o.ticker ?? 'asset';
  const youBuy = o.side === 'sell'; // the maker sells the asset
  const stepIdx = view === 'review' ? 0 : view === 'approve' ? 1 : state === 'awaiting_maker' ? 2 : state === 'signing' ? 3 : state === 'broadcast' ? 4 : state === 'settled' ? 6 : 0;
  const shownProblem: Failure | undefined =
    view === 'problem'
      ? problem
      : state === 'failed'
        ? failure(record?.error?.code ?? 'internal', record?.error?.message)
        : state === 'expired'
          ? failure('expired', record?.error?.message)
          : undefined;

  const title =
    shownProblem ? 'Swap not completed'
    : view === 'review' ? 'Review order'
    : view === 'approve' ? 'Approve in your wallet'
    : state === 'awaiting_maker' ? 'Waiting for the maker'
    : state === 'signing' ? 'Signing together'
    : state === 'broadcast' ? 'Sent to the network'
    : state === 'settled' ? 'Swap complete'
    : 'Swap';

  const announce = `${title}.`;

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-end justify-center sm:items-center sm:p-4">
      <div className="absolute inset-0 animate-fade-in bg-black/70 backdrop-blur-sm" onClick={() => (running ? setHint('The swap is running in your wallet. Use “Close window” to hide this; it keeps going.') : requestClose())} aria-hidden />
      <div
        ref={dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={liveId}
        tabIndex={-1}
        className="relative flex max-h-[94dvh] w-full animate-slide-up flex-col overflow-hidden rounded-t-3xl border border-border-subtle bg-surface-overlay shadow-popover outline-none sm:max-w-[34rem] sm:rounded-3xl"
      >
        <header className="flex items-start gap-3 px-5 pb-3 pt-5">
          <div className="min-w-0 flex-1">
            <div className="text-tiny uppercase tracking-[0.18em] text-content-tertiary">
              {youBuy ? 'Buy' : 'Sell'} {ticker} · peer to peer
            </div>
            <h2 id={titleId} data-autofocus tabIndex={-1} className="mt-0.5 text-title font-bold outline-none">{title}</h2>
          </div>
          <button type="button" onClick={requestClose} className="rounded-lg p-1.5 text-content-secondary hover:bg-surface-elevated hover:text-content-primary" aria-label={running ? 'Close window (the swap keeps running)' : view === 'approve' ? 'Back to review' : 'Close'}>
            <Icon name="x" size={18} />
          </button>
        </header>

        {!shownProblem && <Stepper index={stepIdx} />}

        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5 pt-3">
          <p id={liveId} className="sr-only" aria-live="polite">{announce}</p>

          {shownProblem ? (
            <ProblemView problem={shownProblem} record={record} onClose={onClose} onRetry={shownProblem.retry && !existing ? () => { setProblem(undefined); setSwapId(undefined); setView('review'); } : undefined} />
          ) : view === 'review' ? (
            <ReviewView order={o} checks={checks} nowSec={nowSec} />
          ) : view === 'approve' ? (
            <ApproveView />
          ) : state === 'settled' && record ? (
            <DoneView record={record} />
          ) : (
            <ProgressView record={record} order={o} now={now} />
          )}

          {hint && <p className="mt-3 text-caption text-content-secondary" role="status">{hint}</p>}
        </div>

        {!shownProblem && (
          <footer className="flex flex-col-reverse gap-2 border-t border-border-subtle bg-surface-overlay px-5 py-4 sm:flex-row sm:justify-end">
            {view === 'review' && (
              <>
                <button type="button" className={btnSecondary} onClick={onClose}>Cancel</button>
                <button type="button" className={btnPrimary} onClick={() => void proceed()} disabled={blockers.length > 0 || connecting || wallet.status === 'connecting'}>
                  {connecting || wallet.status === 'connecting' ? <><Spinner /> Approve the connection in your wallet…</>
                    : wallet.status !== 'connected' ? 'Connect wallet to continue'
                    : 'Continue in wallet'}
                </button>
              </>
            )}
            {view === 'approve' && (
              <button type="button" className={btnSecondary} onClick={cancelApproval}>Back to review</button>
            )}
            {running && <button type="button" className={btnSecondary} onClick={onClose}>Close window</button>}
            {state === 'settled' && <button type="button" className={btnPrimary} onClick={onClose}>Done</button>}
          </footer>
        )}
      </div>
    </div>,
    document.body,
  );
}

function Stepper({ index }: { index: number }) {
  return (
    <div className="px-5 pb-1" aria-label={`Step ${Math.min(index + 1, STEPS.length)} of ${STEPS.length}`} role="group">
      <ol className="flex items-center gap-1.5">
        {STEPS.map((s, i) => {
          const done = i < index;
          const current = i === index;
          return (
            <li key={s} className="flex min-w-0 flex-1 flex-col gap-1.5" aria-current={current ? 'step' : undefined}>
              <span className={clsx('h-1 rounded-full transition-colors', done ? 'bg-primary' : current ? 'animate-pulse bg-primary/70' : 'bg-surface-high')} />
              <span className={clsx('truncate text-[10px] leading-3', current ? 'font-bold text-content-primary' : done ? 'text-content-secondary' : 'text-content-tertiary', !current && 'max-sm:hidden')}>{s}</span>
            </li>
          );
        })}
      </ol>
      <div className="mt-1 text-[10px] text-content-tertiary sm:hidden">Step {Math.min(index + 1, STEPS.length)} of {STEPS.length}: {STEPS[Math.min(index, STEPS.length - 1)]}</div>
    </div>
  );
}

function Row({ label, hint, children, strong }: { label: ReactNode; hint?: string; children: ReactNode; strong?: boolean }) {
  return (
    <div className={clsx('flex items-baseline justify-between gap-4 py-1.5 text-caption', strong && 'font-bold')}>
      <dt className="text-content-secondary" title={hint}>{label}</dt>
      <dd className="min-w-0 text-right tabular-nums text-content-primary">{children}</dd>
    </div>
  );
}

function Leg({ label, value, sub, tone }: { label: string; value: string; sub?: string; tone: 'pay' | 'get' }) {
  return (
    <div className={clsx('min-w-0 flex-1 rounded-2xl border px-4 py-3', tone === 'get' ? 'border-primary/30 bg-primary/10' : 'border-border-subtle bg-surface-card/5')}>
      <div className="text-tiny text-content-tertiary">{label}</div>
      <div className="mt-0.5 break-words text-subhead font-bold tabular-nums">{value}</div>
      {sub && <div className="text-tiny text-content-tertiary">{sub}</div>}
    </div>
  );
}

function ReviewView({ order: o, checks, nowSec }: { order: OrderSnapshot; checks: ReturnType<typeof preflight>; nowSec: number }) {
  const wallet = useWebRgb();
  const youBuy = o.side === 'sell';
  const ticker = o.ticker ?? 'asset';
  const units = fmtUnits(o.amount, o.precision);
  const unit = o.amount > 0 ? o.priceSat / (o.amount / 10 ** o.precision) : null;
  const platform = o.platformFeeSat;
  const network = o.networkFeeSat;
  const feesKnown = platform !== undefined && network !== undefined;
  const fees = (platform ?? 0) + (network ?? 0);
  const left = o.expiry - nowSec;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-stretch">
        <Leg tone="pay" label="You pay" value={youBuy ? sat(o.priceSat) : `${units} ${ticker}`} sub={youBuy ? 'plus fees below' : 'asset leaves your wallet'} />
        <div className="hidden items-center text-content-tertiary sm:flex"><Icon name="swap" size={18} className="rotate-90" /></div>
        <Leg tone="get" label="You receive" value={youBuy ? `${units} ${ticker}` : sat(o.priceSat)} sub={youBuy ? 'the whole order, all or nothing' : 'before fees below'} />
      </div>

      <dl className="rounded-2xl border border-border-subtle px-4 py-2 divide-y divide-border-subtle [&>div]:border-border-subtle">
        <Row label="Price per unit" hint="BTC leg divided by the asset amount">{unit ? `${fmtPrice(unit)} sat / ${ticker}` : '—'}</Row>
        <Row label="BTC leg" hint="The price the maker set for the whole order">{sat(o.priceSat)}</Row>
        <Row label="Platform fee" hint="Paid to KaleidoSwap; the wallet only accepts the fee its policy sets">{platform !== undefined ? sat(platform) : 'shown in your wallet'}</Row>
        <Row label="Network fee" hint="Bitcoin miner fee for the swap transaction">{network !== undefined ? sat(network) : 'shown in your wallet'}</Row>
        {o.rgbOutputSat !== undefined && (
          <Row label="RGB output" hint="Sats carried by the coin that holds the asset. Not a fee; when buying, these sats are included in the required funding.">{sat(o.rgbOutputSat)}</Row>
        )}
        {feesKnown && (
          <Row strong label={youBuy ? 'BTC needed for this swap' : 'BTC you receive after fees'} hint={youBuy ? 'BTC leg + platform fee + network fee + RGB output' : 'BTC leg minus platform fee and network fee'}>
            {youBuy ? sat(o.priceSat + fees + (o.rgbOutputSat ?? 0)) : sat(Math.max(0, o.priceSat - fees))}
          </Row>
        )}
      </dl>

      <dl className="rounded-2xl border border-border-subtle px-4 py-2 divide-y divide-border-subtle">
        <Row label="Maker" hint={o.maker}>
          <CopyValue value={o.maker} shown={shortId(o.maker, 8, 6)} className="text-content-primary" />
        </Row>
        <Row label="Maker status">
          <span className="inline-flex flex-wrap items-center justify-end gap-1.5">
            <Pill tone="warning">Not verified</Pill>
            <span className="hidden text-content-tertiary sm:inline">online status is not published</span>
          </span>
        </Row>
        <Row label="Order expires in" hint={new Date(o.expiry * 1000).toLocaleString()}>
          <span className={clsx('inline-flex items-center gap-1.5', left < 600 && 'text-status-warning')}><Icon name="clock" size={13} />{left > 0 ? fmtLeft(o.expiry, nowSec) : 'expired'}</span>
        </Row>
        <Row label="Network">{o.network}</Row>
        <div className="py-2 text-caption">
          <div className="flex items-baseline justify-between gap-4">
            <dt className="text-content-secondary">Asset</dt>
            <dd className="text-right text-content-primary">{ticker}{o.name ? <span className="text-content-tertiary"> · {o.name}</span> : null}</dd>
          </div>
          <dd className="mt-1 break-all font-mono text-tiny text-content-secondary">
            <CopyValue value={o.assetId} shown={<span className="break-all text-left">{o.assetId}</span>} className="items-start" />
          </dd>
          <p className="mt-1.5 text-tiny text-content-tertiary">The name and ticker are the maker’s own claim. The asset id above is what identifies the asset.</p>
        </div>
      </dl>

      {wallet.error && wallet.status !== 'connected' && <Notice tone="warning">{wallet.error}</Notice>}
      {checks.map((c, i) => (
        <Notice key={i} tone={c.level === 'block' ? 'danger' : 'warning'}>{c.text}</Notice>
      ))}

      <div className="flex items-start gap-2.5 rounded-xl bg-surface-card/5 px-3 py-2.5 text-caption text-content-secondary">
        <Icon name="shield" size={16} className="mt-0.5 text-primary" />
        <p>
          This page cannot move funds. Next, <strong className="font-medium text-content-primary">your KaleidoSwap extension opens its own approval window</strong> with these terms; nothing happens until you approve there. The maker must be online to finish the swap.
        </p>
      </div>
    </div>
  );
}

function ApproveView() {
  return (
    <div className="flex flex-col items-center gap-4 py-4 text-center">
      <div className="relative flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/15 text-primary">
        <Icon name="wallet" size={28} />
        <Spinner size={72} className="absolute -inset-1 text-primary/60" />
      </div>
      <div>
        <p className="text-body font-medium">Check the approval window from your KaleidoSwap extension</p>
        <p className="mx-auto mt-1 max-w-sm text-caption text-content-secondary">
          It shows the same terms. Approve there to send the request to the maker, or reject there to stop.
        </p>
      </div>
      <ul className="w-full list-disc rounded-xl bg-surface-card/5 py-3 pl-8 pr-4 text-left text-caption text-content-secondary">
        <li>Don’t see it? Click the KaleidoSwap icon in the browser toolbar, or allow pop-ups for the extension.</li>
        <li>Wallet locked? Unlock it from the same icon first.</li>
        <li>Going back here only stops this page from waiting. If you still approve in the extension, the swap starts and appears under My swaps.</li>
      </ul>
    </div>
  );
}

function ProgressView({ record, order, now }: { record?: SwapRecord; order: OrderSnapshot; now: number }) {
  const state = record?.state ?? 'awaiting_maker';
  const elapsed = record ? (now - record.startedAt) / 1000 : 0;
  const left = MAKER_WAIT_SECONDS - elapsed;
  const ticker = order.ticker ?? 'asset';
  return (
    <div className="flex flex-col gap-4">
      {state === 'awaiting_maker' && (
        <>
          <div className="rounded-2xl border border-border-subtle bg-surface-card/5 p-4">
            <div className="flex items-center gap-3">
              <Spinner size={22} className="text-primary" />
              <div className="min-w-0 flex-1">
                <p className="text-body font-medium">Request sent. Waiting for the maker to answer.</p>
                <p className="text-caption text-content-secondary">The maker has to be online to co-sign the swap.</p>
              </div>
            </div>
            <div className="mt-4 flex items-baseline justify-between text-caption tabular-nums">
              <span className="text-content-secondary">Waiting <strong className="font-mono text-content-primary">{fmtClock(elapsed)}</strong></span>
              <span className={clsx(left > 20 ? 'text-content-secondary' : 'text-status-warning')}>
                {left > 0 ? `gives up in ${fmtClock(left)}` : 'limit reached; waiting for the wallet to confirm the outcome'}
              </span>
            </div>
            <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-surface-high" role="progressbar" aria-valuemin={0} aria-valuemax={MAKER_WAIT_SECONDS} aria-valuenow={Math.min(MAKER_WAIT_SECONDS, Math.floor(elapsed))} aria-label="Time waiting for the maker">
              <div className={clsx('h-full rounded-full transition-[width] duration-1000 ease-linear', left > 20 ? 'bg-primary' : 'bg-status-warning')} style={{ width: `${Math.min(100, (elapsed / MAKER_WAIT_SECONDS) * 100)}%` }} />
            </div>
          </div>
          <Notice tone="info">
            Nothing is spent while you wait. The transaction is only sent once both you and the maker have signed. If the maker does not answer in two minutes the wallet withdraws the request.
          </Notice>
        </>
      )}
      {state === 'signing' && (
        <div className="flex items-center gap-3 rounded-2xl border border-border-subtle bg-surface-card/5 p-4">
          <Spinner size={22} className="text-primary" />
          <div>
            <p className="text-body font-medium">The maker answered. Your wallet is signing or waiting for the broadcast.</p>
            <p className="text-caption text-content-secondary">If the connection drops after signing, the swap may still complete. Reconnect and reopen this swap; do not start another one to replace it.</p>
          </div>
        </div>
      )}
      {state === 'broadcast' && (
        <>
          <div className="flex items-center gap-3 rounded-2xl border border-border-subtle bg-surface-card/5 p-4">
            <Spinner size={22} className="text-primary" />
            <div>
              <p className="text-body font-medium">Transaction broadcast. Waiting for a confirmation.</p>
              <p className="text-caption text-content-secondary">It is on the network now, so it cannot be cancelled. This can take a few minutes{record?.confirmations ? ` · ${record.confirmations} confirmation${record.confirmations === 1 ? '' : 's'}` : ''}.</p>
            </div>
          </div>
          {record?.txid && <TxLink txid={record.txid} />}
        </>
      )}
      <dl className="rounded-2xl border border-border-subtle px-4 py-2 divide-y divide-border-subtle">
        <Row label={order.side === 'sell' ? 'You buy' : 'You sell'}>{fmtUnits(order.amount, order.precision)} {ticker}</Row>
        <Row label="For">{sat(order.priceSat)}</Row>
        {record && <Row label="Swap id"><span className="font-mono text-tiny" title={record.swapId}>{shortId(record.swapId, 8, 6)}</span></Row>}
      </dl>
      <p className="text-caption text-content-tertiary">You can close this window. The swap keeps running in your wallet and stays listed under My swaps.</p>
    </div>
  );
}

function TxLink({ txid }: { txid: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-xl bg-surface-card/5 px-3 py-2.5 text-caption">
      <div className="min-w-0">
        <div className="text-tiny text-content-tertiary">Transaction id</div>
        <CopyValue value={txid} shown={<span className="truncate">{shortId(txid, 12, 8)}</span>} className="max-w-full text-content-primary" />
      </div>
      <a className="inline-flex shrink-0 items-center gap-1 rounded-lg px-2 py-1 text-primary hover:bg-primary/10" href={`${EXPLORER_TX}${txid}`} target="_blank" rel="noreferrer">
        View on explorer <Icon name="external" size={13} />
      </a>
    </div>
  );
}

function DoneView({ record }: { record: SwapRecord }) {
  const wallet = useWebRgb();
  const o = record.order;
  const ticker = o.ticker ?? 'asset';
  const youBuy = o.side === 'sell';
  const assetNow = wallet.assets.find((a) => a.id === o.assetId)?.balance;
  const satNow = wallet.balance?.spendable;
  const delta = (now: number | undefined, before: number | null | undefined, fmt: (n: number) => string) => {
    if (now === undefined || before === undefined || before === null) return null;
    const d = now - before;
    return d === 0 ? <span className="text-content-tertiary"> (not updated yet)</span> : <span className={d > 0 ? 'text-status-success' : 'text-status-danger'}> ({d > 0 ? '+' : '−'}{fmt(Math.abs(d))})</span>;
  };
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col items-center gap-2 py-2 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-full bg-primary/15 text-primary"><Icon name="check" size={28} /></div>
        <p className="text-body font-bold">
          You {youBuy ? 'bought' : 'sold'} {fmtUnits(o.amount, o.precision)} {ticker}
        </p>
        <p className="text-caption text-content-secondary">for {sat(o.priceSat)}, confirmed on the network.</p>
      </div>
      {record.txid && <TxLink txid={record.txid} />}
      <dl className="rounded-2xl border border-border-subtle px-4 py-2 divide-y divide-border-subtle">
        <Row label="BTC balance" hint="Spendable, as the wallet reports it">
          {satNow !== undefined ? <>{sat(satNow)}{delta(satNow, record.before?.sat, (n) => sat(n))}</> : '—'}
        </Row>
        <Row label={`${ticker} balance`}>
          {assetNow !== undefined ? <>{fmtUnits(assetNow, o.precision)}{delta(assetNow, record.before?.asset, (n) => fmtUnits(n, o.precision))}</> : '—'}
        </Row>
      </dl>
      <p className="text-tiny text-content-tertiary">Balances come from your wallet and may take a moment to reflect the swap.</p>
    </div>
  );
}

function ProblemView({ problem, record, onClose, onRetry }: { problem: Failure; record?: SwapRecord; onClose: () => void; onRetry?: () => void }) {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start gap-3 rounded-2xl border border-status-danger/30 bg-status-danger-subtle p-4">
        <Icon name="alert" size={22} className="mt-0.5 text-status-danger" />
        <div className="min-w-0">
          <p className="text-body font-bold text-status-danger" role="alert">{problem.title}</p>
          <p className="mt-0.5 text-caption text-content-primary">{problem.reason}</p>
        </div>
      </div>
      <div className="rounded-xl bg-surface-card/5 px-4 py-3 text-caption">
        <div className="text-tiny uppercase tracking-[0.18em] text-content-tertiary">What to do</div>
        <p className="mt-1 text-content-primary">{problem.todo}</p>
      </div>
      {problem.detail && (
        <p className="break-words text-tiny text-content-tertiary">
          Wallet message: <span className="font-mono">{problem.code}</span> — {problem.detail}
        </p>
      )}
      {record?.txid && <TxLink txid={record.txid} />}
      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <button type="button" className={btnSecondary} onClick={onClose}>Close</button>
        {onRetry && <button type="button" className={btnPrimary} onClick={onRetry}>Review and try again</button>}
      </div>
    </div>
  );
}
