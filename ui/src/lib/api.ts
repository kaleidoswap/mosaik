import type {
  ApiState,
  OrderbookLevel,
  Trade,
  Candle,
  DepthPoint,
  OpenOrder,
  Quote,
} from '../types'

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || `HTTP ${res.status}`)
  }
  return res.json() as Promise<T>
}

export async function fetchState(): Promise<ApiState> {
  return apiFetch<ApiState>('/api/state')
}

export async function fetchOrderbook(pair: string): Promise<{ bids: OrderbookLevel[]; asks: OrderbookLevel[] }> {
  return apiFetch(`/api/orderbook?pair=${pair}`)
}

export async function fetchTrades(pair: string, limit = 50): Promise<Trade[]> {
  return apiFetch(`/api/trades?pair=${pair}&limit=${limit}`)
}

export async function fetchChart(pair: string, tf: string, limit = 200): Promise<Candle[]> {
  return apiFetch(`/api/chart?pair=${pair}&tf=${tf}&limit=${limit}`)
}

export async function fetchDepth(pair: string): Promise<DepthPoint[]> {
  return apiFetch(`/api/depth?pair=${pair}`)
}

export async function fetchUserOrders(): Promise<OpenOrder[]> {
  return apiFetch('/api/user/orders')
}

export async function cancelOrder(index: number): Promise<{ ok: boolean; txid?: string }> {
  return apiFetch('/api/cancel', {
    method: 'POST',
    body: JSON.stringify({ index }),
  })
}

export async function fetchQuote(from: string, to: string, fromAmount: number): Promise<Quote> {
  return apiFetch('/api/rfq/quote', {
    method: 'POST',
    body: JSON.stringify({ from, to, from_amount: fromAmount }),
  })
}

export async function confirmQuote(quoteId: number): Promise<{ txid: string }> {
  return apiFetch('/api/rfq/confirm', {
    method: 'POST',
    body: JSON.stringify({ quote_id: quoteId }),
  })
}

export async function makeOffer(
  lock: string,
  want: string,
  amountA: number,
  amountB: number,
  timeout?: number
): Promise<{ ok: boolean; index: number; outpoint: string }> {
  return apiFetch('/api/make-offer', {
    method: 'POST',
    body: JSON.stringify({ lock, want, amount_a: amountA, amount_b: amountB, timeout: timeout ?? 500 }),
  })
}

export async function fundMaker(): Promise<{ ok: boolean }> {
  return apiFetch('/api/fund/maker', { method: 'POST' })
}

export async function fundTaker(): Promise<{ ok: boolean }> {
  return apiFetch('/api/fund/taker', { method: 'POST' })
}
