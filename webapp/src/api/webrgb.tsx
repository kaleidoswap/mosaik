/**
 * Sign-in with a WebRGB wallet (the browser extension). The page never holds keys: "connecting"
 * is `enable()` on the provider, which asks the user once per site, and everything shown about the
 * wallet is read from it. Providers are found two ways: the `rgb:announceProvider` discovery event
 * and `window.rgb`, which a content script may define after the page's first render.
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { RgbTakerProvider } from './rgbWallet';

export interface WalletAsset {
  id: string;
  ticker: string;
  name: string;
  balance: number;
  precision: number;
}
export interface WalletBalance {
  spendable: number;
  total: number;
  rgbReserved?: number;
  freeColorableUtxos: number | null;
}

export function toBalance(bal: { swapSpendable?: number; vanilla: { spendable: number; settled: number; future: number }; colored?: { settled: number }; freeColorableUtxos: number | null }): WalletBalance {
  return {
    // P2P inputs require a confirmation; pending change cannot fund a swap yet.
    spendable: bal.swapSpendable ?? Math.min(bal.vanilla.spendable, bal.vanilla.settled),
    // rgb-lib's future balance already includes settled coins.
    total: bal.vanilla.future,
    ...(bal.colored ? { rgbReserved: bal.colored.settled } : {}),
    freeColorableUtxos: bal.freeColorableUtxos,
  };
}
export type WalletStatus = 'absent' | 'ready' | 'connecting' | 'connected';

interface WalletApi {
  status: WalletStatus;
  provider?: RgbTakerProvider;
  walletName?: string;
  network?: string;
  walletLabel?: string;
  balance?: WalletBalance;
  assets: WalletAsset[];
  error?: string;
  /** Why the wallet cannot take peer-to-peer orders right now, when it connected but cannot. */
  p2pProblem?: string;
  canTake: boolean;
  connect: () => Promise<boolean>;
  disconnect: () => void;
  refresh: () => Promise<void>;
}

type Provider = RgbTakerProvider & {
  getInfo?: () => Promise<{ network?: string }>;
  getBtcBalance?: () => Promise<{ swapSpendable?: number; colored?: { settled: number }; vanilla: { spendable: number; future: number; settled: number }; freeColorableUtxos: number | null }>;
  listAssets?: () => Promise<unknown[]>;
};

const FLAG = 'mosaik.webrgb.connected';
const store = {
  get: () => { try { return localStorage.getItem(FLAG) === '1'; } catch { return false; } },
  set: (v: boolean) => { try { v ? localStorage.setItem(FLAG, '1') : localStorage.removeItem(FLAG); } catch { /* private mode */ } },
};

const num = (v: unknown): number => (typeof v === 'number' && Number.isFinite(v) ? v : Number(v) || 0);
const str = (v: unknown): string => (typeof v === 'string' ? v : '');

export function toAsset(raw: unknown): WalletAsset | undefined {
  const o = (raw ?? {}) as Record<string, unknown>;
  const id = str(o.assetId) || str(o.asset_id) || str(o.id);
  if (!id) return undefined;
  const b = o.balance;
  const balance = typeof b === 'object' && b ? num((b as Record<string, unknown>).spendable ?? (b as Record<string, unknown>).settled) : num(b);
  return { id, ticker: str(o.ticker) || 'asset', name: str(o.name), balance, precision: num(o.precision) };
}

const Ctx = createContext<WalletApi | null>(null);

export function WebRgbProvider({ children }: { children: ReactNode }) {
  const [provider, setProvider] = useState<Provider>();
  const [walletName, setWalletName] = useState<string>();
  const [status, setStatus] = useState<WalletStatus>('absent');
  const [network, setNetwork] = useState<string>();
  const [walletLabel, setWalletLabel] = useState<string>();
  const [balance, setBalance] = useState<WalletBalance>();
  const [assets, setAssets] = useState<WalletAsset[]>([]);
  const [error, setError] = useState<string>();
  const [p2pProblem, setP2pProblem] = useState<string>();
  const found = useRef(false);

  useEffect(() => {
    const adopt = (p: Provider, name?: string) => {
      if (found.current) return;
      found.current = true;
      setProvider(p);
      setWalletName(name);
      setStatus((s) => (s === 'absent' ? 'ready' : s));
    };
    const onAnnounce = (e: Event) => {
      const d = (e as CustomEvent<{ info?: { name?: string }; provider?: Provider }>).detail;
      if (d?.provider) adopt(d.provider, d.info?.name);
    };
    window.addEventListener('rgb:announceProvider', onAnnounce);
    window.dispatchEvent(new CustomEvent('rgb:requestProvider'));
    let tries = 0;
    const poll = window.setInterval(() => {
      if (window.rgb) adopt(window.rgb as Provider);
      if (found.current || ++tries > 40) window.clearInterval(poll);
    }, 250);
    if (window.rgb) adopt(window.rgb as Provider);
    return () => {
      window.removeEventListener('rgb:announceProvider', onAnnounce);
      window.clearInterval(poll);
    };
  }, []);

  const refresh = useCallback(async () => {
    if (!provider) return;
    try {
      let p2pNet: string | undefined;
      setP2pProblem(undefined);
      try {
        p2pNet = await provider.p2pSwap?.network();
      } catch (e) {
        const m = e instanceof Error ? e.message : String(e);
        setP2pProblem(
          /no rgb wallet is connected|no active protocol|locked/i.test(m)
            ? 'locked'
            : m,
        );
      }
      const [info, bal, list] = await Promise.all([
        provider.getInfo?.(),
        provider.getBtcBalance?.(),
        provider.listAssets?.(),
      ]);
      // Only the swap network is comparable with the orders' (rgb-lib names such as `signetcustom`);
      // the wallet's own label (`mutinynet`) is not, so it is shown but never compared.
      setNetwork(p2pNet);
      setWalletLabel(info?.network);
      if (bal) {
        setBalance(toBalance(bal));
      }
      setAssets((list ?? []).map(toAsset).filter((a): a is WalletAsset => !!a));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [provider]);

  const connect = useCallback(async () => {
    if (!provider) return false;
    setError(undefined);
    setStatus('connecting');
    try {
      await provider.enable?.();
      setStatus('connected');
      store.set(true);
      await refresh();
      return true;
    } catch (e) {
      const m = e instanceof Error ? e.message : String(e);
      setError(/reject|declin|denied|cancel/i.test(m) ? 'Connection declined in the wallet.' : m);
      setStatus('ready');
      return false;
    }
  }, [provider, refresh]);

  const disconnect = useCallback(() => {
    store.set(false);
    setStatus(provider ? 'ready' : 'absent');
    setBalance(undefined);
    setAssets([]);
  }, [provider]);

  // A site the user connected before reconnects when the wallet is there; the wallet only asks again if it forgot.
  useEffect(() => {
    if (provider && status === 'ready' && store.get()) void connect();
  }, [provider, status, connect]);

  // A locked wallet is not an error: look again until the user unlocks it in the extension.
  useEffect(() => {
    if (status !== 'connected' || p2pProblem !== 'locked') return;
    const t = window.setInterval(() => void refresh(), 3_000);
    return () => window.clearInterval(t);
  }, [status, p2pProblem, refresh]);

  // Keep balances fresh while connected.
  useEffect(() => {
    if (status !== 'connected') return;
    const t = window.setInterval(() => void refresh(), 30_000);
    return () => window.clearInterval(t);
  }, [status, refresh]);

  const value = useMemo<WalletApi>(
    () => ({ status, provider, walletName, network, walletLabel, balance, assets, error, p2pProblem, canTake: !!provider?.p2pSwap, connect, disconnect, refresh }),
    [status, provider, walletName, network, walletLabel, balance, assets, error, p2pProblem, connect, disconnect, refresh],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useWebRgb(): WalletApi {
  const v = useContext(Ctx);
  if (!v) throw new Error('useWebRgb outside WebRgbProvider');
  return v;
}
