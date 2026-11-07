/**
 * What a failed swap means for the user. The provider reports `{ code, message }`; the message is the
 * wallet's own wording and is shown as detail, while the title and the advice come from here.
 */
import type { OrderSnapshot } from './swaps';
import type { WalletAsset, WalletBalance } from './webrgb';

export interface Failure {
  code: string;
  title: string;
  /** What happened, in one sentence. */
  reason: string;
  /** What to do next. */
  todo: string;
  /** Trying the same order again can work. */
  retry: boolean;
  /** The wallet's own wording, when it gave one. */
  detail?: string;
}

const CATALOGUE: Record<string, Omit<Failure, 'code' | 'detail'>> = {
  order_gone: {
    title: 'The order is gone',
    reason: 'Someone else took it, or the maker withdrew it, before your request arrived.',
    todo: 'Nothing was spent. Pick another order from the book.',
    retry: false,
  },
  order_changed: {
    title: 'The order changed',
    reason: 'The maker replaced this order with different terms after you opened it.',
    todo: 'Nothing was spent. Go back to the book and review the updated order.',
    retry: false,
  },
  wrong_network: {
    title: 'Wrong network',
    reason: 'Your wallet is on a different network than this order.',
    todo: 'Switch the extension to the same network as the order, then try again.',
    retry: true,
  },
  fee_policy: {
    title: 'Your wallet rejected the fees',
    reason: 'The fees or the fee recipient in this order are not what your wallet accepts.',
    todo: 'Nothing was spent. Take another order; this one cannot be taken from this wallet.',
    retry: false,
  },
  untrusted_proxy: {
    title: 'Unknown RGB proxy',
    reason: 'The order points to an RGB proxy server your wallet does not trust.',
    todo: 'Nothing was spent. Take another order.',
    retry: false,
  },
  insufficient_funds: {
    title: 'Not enough funds',
    reason: 'Your wallet cannot cover this order and its fees.',
    todo: 'Add funds in the extension, or take a smaller order, then try again.',
    retry: true,
  },
  no_colored_coin: {
    title: 'No free coin for RGB',
    reason: 'RGB assets travel on a dedicated coin, and your wallet has none free.',
    todo: 'Create a coin for RGB in the extension, wait for it to confirm, then try again.',
    retry: true,
  },
  maker_unreachable: {
    title: 'The maker is not reachable',
    reason: 'The maker did not receive your request. They may be offline.',
    todo: 'Nothing was spent. Try again later or take another maker’s order.',
    retry: true,
  },
  maker_aborted: {
    title: 'The maker stopped the swap',
    reason: 'The maker cancelled before the transaction was signed by both sides.',
    todo: 'Nothing was spent. Try again, or take another maker’s order.',
    retry: true,
  },
  user_declined: {
    title: 'You declined in the wallet',
    reason: 'The approval was rejected in the extension.',
    todo: 'Nothing was spent. You can review the order and try again.',
    retry: true,
  },
  expired: {
    title: 'The maker did not answer in time',
    reason: 'The wallet waited for two minutes and withdrew the request.',
    todo: 'Nothing was spent. The maker is probably offline: try again later or take another order.',
    retry: true,
  },
  locked: {
    title: 'Your wallet is locked',
    reason: 'The extension cannot approve a swap while it is locked.',
    todo: 'Click the KaleidoSwap icon in the browser toolbar, unlock the wallet, then try again.',
    retry: true,
  },
  popup_closed: {
    title: 'The approval window was closed',
    reason: 'The extension’s approval window closed before you approved.',
    todo: 'Nothing was spent. Review the order and continue again.',
    retry: true,
  },
  internal: {
    title: 'The wallet hit an error',
    reason: 'Something went wrong inside the extension.',
    todo: 'Check that your wallet is unlocked and on the right network, then try again. Nothing is spent until both sides have signed.',
    retry: true,
  },
};

export function failure(code: string, message?: string): Failure {
  const base = CATALOGUE[code] ?? CATALOGUE.internal!;
  return { code, ...base, detail: message?.trim() || undefined };
}

const KNOWN = Object.keys(CATALOGUE);

/** A rejection from `take()` as a failure. The extension puts the code first: `fee_policy: …`. */
export function failureFromError(e: unknown): Failure {
  const m = e instanceof Error ? e.message : String(e);
  const lead = /^\s*([a-z_]+)\s*:\s*([\s\S]*)$/.exec(m);
  if (lead && KNOWN.includes(lead[1]!)) return failure(lead[1]!, lead[2]);
  if (/popup_closed|approval window was closed|window was closed/i.test(m)) return failure('popup_closed', m);
  if (/declin|reject|denied|cancel/i.test(m)) return failure('user_declined', m);
  if (/no rgb wallet is connected|no active protocol|locked/i.test(m)) return failure('locked', m);
  return failure('internal', m);
}

export interface Preflight {
  level: 'block' | 'warn';
  text: string;
}

export interface WalletView {
  connected: boolean;
  canTake: boolean;
  p2pProblem?: string;
  network?: string;
  balance?: WalletBalance;
  assets: WalletAsset[];
}

/** What the page can already tell from what the wallet reports. The wallet still decides. */
export function preflight(o: OrderSnapshot, w: WalletView, nowSec: number): Preflight[] {
  const out: Preflight[] = [];
  if (o.expiry <= nowSec) out.push({ level: 'block', text: 'This order has expired.' });
  if (!w.canTake) {
    out.push({ level: 'block', text: 'Your wallet does not support peer-to-peer swaps. Install or update the KaleidoSwap extension.' });
    return out;
  }
  if (!w.connected) return out;
  if (w.p2pProblem === 'locked') {
    out.push({ level: 'block', text: 'Your wallet is locked. Unlock it from the extension icon in the toolbar.' });
    return out;
  }
  if (w.p2pProblem) {
    out.push({ level: 'block', text: `Your wallet cannot take orders: ${w.p2pProblem}` });
    return out;
  }
  if (w.network && w.network !== o.network) {
    out.push({ level: 'block', text: `Your wallet swaps on ${w.network}; this order is on ${o.network}. Switch network in the extension.` });
  }
  const fees = (o.platformFeeSat ?? 0) + (o.networkFeeSat ?? 0);
  const spendable = w.balance?.spendable;
  if (spendable !== undefined) {
    const needSat = o.side === 'sell' ? o.priceSat + fees : fees;
    if (spendable < needSat) {
      out.push({
        level: 'block',
        text: `You need about ${needSat.toLocaleString()} sat and the wallet reports ${spendable.toLocaleString()} sat spendable.`,
      });
    }
  }
  if (o.side === 'buy') {
    const have = w.assets.find((a) => a.id === o.assetId)?.balance ?? 0;
    if (have < o.amount) out.push({ level: 'block', text: 'You do not hold enough of this asset to fill this order.' });
  }
  if (w.balance?.freeColorableUtxos === 0) {
    out.push({
      level: 'warn',
      text: 'No free coin for RGB in your wallet. The wallet will refuse the swap until you create one in the extension.',
    });
  }
  return out;
}
