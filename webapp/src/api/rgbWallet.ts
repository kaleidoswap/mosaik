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

export interface RgbTakerProvider {
  /** The extension can take peer-to-peer RGB orders. */
  p2pSwap?: {
    take(terms: TakeTerms): Promise<{ swapId: string }>;
    status(swapId: string): Promise<{ state: string; txid?: string; error?: string }>;
  };
}

declare global {
  interface Window {
    rgb?: RgbTakerProvider;
  }
}

export const canTakeP2p = (): boolean => typeof window !== 'undefined' && !!window.rgb?.p2pSwap;
