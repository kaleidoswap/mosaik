/**
 * The wallet that takes an RGB order for the user: the browser extension's `window.rgb` provider.
 *
 * The webapp holds no keys. Taking an order is an interactive swap (the maker co-signs and must be
 * online), and the extension signs and broadcasts it after showing the user the terms. These are
 * the calls this page needs from it; until the extension offers them, the page says so and leaves
 * the order book read-only.
 */

export interface TakeTerms {
  orderId: string;
  maker: string;
  assetId: string;
  side: 'sell' | 'buy';
  amount: number;
  priceSat: number;
}

export type SwapState = 'awaiting_maker' | 'signing' | 'broadcast' | 'settled' | 'failed' | 'expired';

export interface SwapStatus {
  swapId: string;
  state: SwapState;
  confirmations?: number;
  txid?: string;
  error?: { code: string; message: string };
}

export interface RgbTakerProvider {
  enable?(): Promise<unknown>;
  /** The extension can take peer-to-peer RGB orders. */
  p2pSwap?: {
    /** The network the wallet is on, so the page can hide other books. */
    network(): Promise<string>;
    /** Ask the user to take an order. Resolves when the request is sent, not when the swap ends. */
    take(terms: TakeTerms): Promise<{ swapId: string }>;
    status(swapId: string): Promise<SwapStatus>;
    /** Reports one swap's changes while this page is open; the wallet runs the swap either way. */
    onStatus(swapId: string, cb: (s: SwapStatus) => void): () => void;
  };
}

declare global {
  interface Window {
    rgb?: RgbTakerProvider;
  }
}

export const canTakeP2p = (): boolean => typeof window !== 'undefined' && !!window.rgb?.p2pSwap;

/**
 * Take an order through the extension and follow the swap. Rejects when the user declines or the
 * wallet refuses the order (the reason is first in the message: `fee_policy: …`).
 */
export async function takeOrder(
  terms: TakeTerms,
  onStatus: (s: SwapStatus) => void,
  provider: RgbTakerProvider | undefined = window.rgb,
): Promise<() => void> {
  const rgb = provider;
  if (!rgb?.p2pSwap) throw new Error('The KaleidoSwap extension is needed to take an order');
  await rgb.enable?.();
  const { swapId } = await rgb.p2pSwap.take(terms);
  return rgb.p2pSwap.onStatus(swapId, onStatus);
}
