/**
 * mosaik-api HTTP client. Same-origin in production (nginx proxies /api),
 * proxied through vite in dev. The base URL is rarely set explicitly; if it
 * is, it overrides both.
 */

import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import type {
  ApiState,
  ApiError,
  MakeOfferRequest,
  MakeOfferResponse,
  TakeOfferRequest,
  TakeOfferResponse,
} from './types';

const BASE = import.meta.env.VITE_API_URL ?? '';

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'content-type': 'application/json' },
    ...init,
  });
  if (!res.ok) {
    let msg: string;
    try {
      const body = (await res.json()) as ApiError;
      msg = body.error ?? `HTTP ${res.status}`;
    } catch {
      msg = `HTTP ${res.status}`;
    }
    throw new Error(msg);
  }
  return res.json() as Promise<T>;
}

// ─── Hooks ───────────────────────────────────────────────────────────────────

/** Polls /api/state on a tight interval so the order book / wallet snapshots
 *  feel live. ~2s is well under the regtest mining cadence the demo uses. */
export function useApiState() {
  return useQuery<ApiState>({
    queryKey: ['state'],
    queryFn: () => request<ApiState>('/api/state'),
    refetchInterval: 2_000,
  });
}

export function useFundWallet() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (wallet: string) =>
      request<{ ok: true; wallet: string; funded: string }>('/api/fund', {
        method: 'POST',
        body: JSON.stringify({ wallet }),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['state'] }),
  });
}

export function useMakeOffer() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: MakeOfferRequest) =>
      request<MakeOfferResponse>('/api/make-offer', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['state'] }),
  });
}

export function useTakeOffer() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (req: TakeOfferRequest) =>
      request<TakeOfferResponse>('/api/take-offer', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['state'] }),
  });
}

export function useReclaim() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (index: number) =>
      request<{ ok: true; txid: string }>('/api/reclaim', {
        method: 'POST',
        body: JSON.stringify({ index }),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['state'] }),
  });
}
