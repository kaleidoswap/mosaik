import { create } from 'zustand'
import type {
  ApiState,
  OrderbookLevel,
  Trade,
  Candle,
  DepthPoint,
  OpenOrder,
} from '../types'
import {
  fetchState,
  fetchOrderbook,
  fetchTrades,
  fetchChart,
  fetchDepth,
  fetchUserOrders,
} from '../lib/api'

interface MarketState {
  pair: string
  timeframe: string
  state: ApiState | null
  orderbook: { bids: OrderbookLevel[]; asks: OrderbookLevel[] }
  trades: Trade[]
  candles: Candle[]
  depth: DepthPoint[]
  openOrders: OpenOrder[]
  price: number
  prevPrice: number
  loading: boolean
  error: string | null
  setPair: (p: string) => void
  setTimeframe: (tf: string) => void
  refresh: () => Promise<void>
  refreshChart: () => Promise<void>
}

export const useMarketStore = create<MarketState>((set, get) => ({
  pair: 'BTC-USDT',
  timeframe: '1h',
  state: null,
  orderbook: { bids: [], asks: [] },
  trades: [],
  candles: [],
  depth: [],
  openOrders: [],
  price: 0,
  prevPrice: 0,
  loading: false,
  error: null,

  setPair: (p) => {
    set({ pair: p })
    get().refresh()
    get().refreshChart()
  },

  setTimeframe: (tf) => {
    set({ timeframe: tf })
    get().refreshChart()
  },

  refresh: async () => {
    const { pair } = get()
    set({ loading: true, error: null })
    try {
      const [stateData, bookData, tradesData, depthData, ordersData] = await Promise.all([
        fetchState(),
        fetchOrderbook(pair),
        fetchTrades(pair),
        fetchDepth(pair),
        fetchUserOrders(),
      ])

      const offers = stateData.offers ?? []
      const currentPrice = offers.length > 0
        ? offers[0].amount_b / (offers[0].amount_a || 1)
        : 0

      set({
        state: stateData,
        orderbook: bookData,
        trades: tradesData,
        depth: depthData,
        openOrders: ordersData,
        prevPrice: get().price,
        price: currentPrice,
        loading: false,
      })
    } catch (err: any) {
      set({ error: err.message || 'Refresh failed', loading: false })
    }
  },

  refreshChart: async () => {
    const { pair, timeframe } = get()
    try {
      const candles = await fetchChart(pair, timeframe)
      set({ candles })
    } catch {
      // chart errors are non-fatal
    }
  },
}))
