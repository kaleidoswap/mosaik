import { useState } from 'react';
import clsx from 'clsx';
import {
  useAdvanceSwap,
  useDraftSwap,
  useRefundSwap,
  useSwaps,
} from '../api/client';
import type {
  RefundReason,
  SwapAdvanceAction,
  SwapEnvelope,
  SwapStateLabel,
} from '../api/types';
import { shortId } from '../lib/format';

/**
 * Cross-chain swap board — drives the mosaik-bridge state machine in the SPA.
 *
 * Today this talks to the API's mock orchestrator: draft → publish A →
 * publish B → observe preimage → complete. When the two primitive gaps
 * in docs/CROSS_CHAIN_SWAP.md close, the same component drives real
 * on-chain transitions without any UI change.
 */
export function SwapBoard() {
  const swaps = useSwaps();
  const items = swaps.data?.swaps ?? [];

  return (
    <section className="panel flex flex-col min-h-0">
      <header className="panel-header">
        <h2 className="panel-title">Cross-chain swaps</h2>
        <span className="text-[10px] font-mono text-zinc-500">
          {items.length} in flight · mock orchestrator
        </span>
      </header>

      <SwapDraftForm />

      {items.length === 0 ? (
        <div className="p-6 text-center text-sm text-zinc-500">
          No swaps yet. Draft one above to walk the state machine.
        </div>
      ) : (
        <ul className="divide-y divide-ink-500/40">
          {items.map((s) => (
            <SwapRow key={s.swap_id} swap={s} />
          ))}
        </ul>
      )}
    </section>
  );
}

// ─── Draft form ───────────────────────────────────────────────────────────────

function SwapDraftForm() {
  const [swapId, setSwapId] = useState('');
  const draft = useDraftSwap();

  function submit(e: React.FormEvent) {
    e.preventDefault();
    draft.mutate({ swap_id: swapId || undefined });
    setSwapId('');
  }

  return (
    <form
      onSubmit={submit}
      className="flex items-center gap-2 border-b border-ink-500/40 p-3"
    >
      <input
        className="input flex-1 font-mono !py-1"
        placeholder="swap id (auto-generated if empty)"
        value={swapId}
        onChange={(e) => setSwapId(e.target.value)}
      />
      <button type="submit" className="btn-primary !py-1 !px-3 !text-xs" disabled={draft.isPending}>
        {draft.isPending ? 'Drafting…' : 'Draft swap'}
      </button>
      {draft.error && (
        <span className="text-xs text-mosaic-rose">
          {(draft.error as Error).message}
        </span>
      )}
    </form>
  );
}

// ─── Single-swap row ─────────────────────────────────────────────────────────

function SwapRow({ swap }: { swap: SwapEnvelope }) {
  const stateLabel = swap.state.state;
  const refundReason =
    swap.state.state === 'refunded' ? swap.state.reason : null;

  return (
    <li className="p-3 hover:bg-ink-500/20">
      <div className="mb-2 flex items-center gap-3">
        <span className="font-mono text-sm text-zinc-200">{swap.swap_id}</span>
        <StatePill state={stateLabel} reason={refundReason ?? undefined} />
        <span className="ml-auto text-[10px] font-mono text-zinc-500">
          hash {shortId(swap.preimage_hash, 8, 6)}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-2 font-mono text-xs text-zinc-400 mb-2">
        <LegCard legLabel="A" backend={swap.leg_a.backend} outpoint={swap.leg_a.outpoint} />
        <LegCard legLabel="B" backend={swap.leg_b.backend} outpoint={swap.leg_b.outpoint} />
      </div>

      <SwapActions swap={swap} />
    </li>
  );
}

function LegCard({
  legLabel,
  backend,
  outpoint,
}: {
  legLabel: string;
  backend: string;
  outpoint: string;
}) {
  return (
    <div className="rounded border border-ink-500/40 bg-ink-800/60 px-2 py-1.5">
      <div className="flex items-center justify-between text-[10px] uppercase tracking-wider text-zinc-500">
        <span>Leg {legLabel}</span>
        <span
          className={clsx(
            backend === 'liquid' && 'text-mosaic-cyan',
            backend === 'banco' && 'text-mosaic-violet',
          )}
        >
          {backend}
        </span>
      </div>
      <div className="mt-0.5 truncate text-zinc-300">
        {outpoint || <span className="text-zinc-600">— not funded —</span>}
      </div>
    </div>
  );
}

// ─── State pill ──────────────────────────────────────────────────────────────

function StatePill({
  state,
  reason,
}: {
  state: SwapStateLabel;
  reason?: RefundReason;
}) {
  const colour: Record<SwapStateLabel, string> = {
    drafted: 'bg-zinc-500/20 text-zinc-300',
    half_published: 'bg-mosaic-cyan/15 text-mosaic-cyan',
    both_published: 'bg-mosaic-cyan/25 text-mosaic-cyan',
    revealed: 'bg-mosaic-amber/20 text-mosaic-amber',
    settled: 'bg-mosaic-mint/20 text-mosaic-mint',
    refunded: 'bg-mosaic-rose/20 text-mosaic-rose',
  };
  return (
    <span
      className={clsx(
        'rounded-full px-2 py-0.5 text-[10px] font-mono uppercase tracking-wider ring-1 ring-inset ring-white/5',
        colour[state],
      )}
      title={reason ? `refund reason: ${reason}` : undefined}
    >
      {state.replace(/_/g, ' ')}
      {reason ? ` · ${reason.replace(/_/g, ' ')}` : ''}
    </span>
  );
}

// ─── Action buttons (state-driven) ────────────────────────────────────────────

function SwapActions({ swap }: { swap: SwapEnvelope }) {
  const advance = useAdvanceSwap();
  const refund = useRefundSwap();

  const next = nextAction(swap.state.state);
  const canRefund = !['settled', 'refunded'].includes(swap.state.state);

  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {next && (
        <button
          className="btn-primary !py-1 !px-2 !text-xs"
          disabled={advance.isPending}
          onClick={() => advance.mutate({ id: swap.swap_id, action: next })}
        >
          {labelForAction(next)}
        </button>
      )}
      {canRefund && (
        <button
          className="btn-secondary !py-1 !px-2 !text-xs"
          disabled={refund.isPending}
          onClick={() =>
            refund.mutate({
              id: swap.swap_id,
              reason: refundReasonFor(swap.state.state),
            })
          }
          title="Terminate the swap on the matching refund branch"
        >
          Refund
        </button>
      )}
      {(advance.error || refund.error) && (
        <span className="text-xs text-mosaic-rose">
          {(advance.error ?? refund.error)?.message}
        </span>
      )}
    </div>
  );
}

function nextAction(state: SwapStateLabel): SwapAdvanceAction | null {
  switch (state) {
    case 'drafted':
      return 'publish_leg_a';
    case 'half_published':
      return 'publish_leg_b';
    case 'both_published':
      return 'observe_preimage';
    case 'revealed':
      return 'complete';
    default:
      return null;
  }
}

function labelForAction(a: SwapAdvanceAction): string {
  switch (a) {
    case 'publish_leg_a':
      return 'Publish leg A';
    case 'publish_leg_b':
      return 'Publish leg B';
    case 'observe_preimage':
      return 'Observe preimage';
    case 'complete':
      return 'Complete';
  }
}

function refundReasonFor(state: SwapStateLabel): RefundReason {
  switch (state) {
    case 'drafted':
      return 'never_published';
    case 'half_published':
      return 'partner_never_published';
    case 'both_published':
      return 'no_taker_arrived';
    case 'revealed':
      return 'maker_missed_claim_window';
    default:
      return 'never_published';
  }
}
