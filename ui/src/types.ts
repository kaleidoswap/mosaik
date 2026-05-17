export interface Offer {
  index: number
  outpoint: string
  amount_a: number
  amount_b: number
  lock_name: string
  want_name: string
  price: number
  maker_address: string
  timeout: number
}

export interface OrderbookLevel {
  price: number
  amount: number
  total: number
  side: 'bid' | 'ask'
}

export interface Trade {
  price: number
  amount: number
  side: 'buy' | 'sell'
  time: string
}

export interface Candle {
  time: number
  open: number
  high: number
  low: number
  close: number
  volume: number
}

export interface DepthPoint {
  price: number
  bidDepth: number
  askDepth: number
}

export interface OpenOrder {
  id: number
  type: 'BUY' | 'SELL'
  pair: string
  price: number
  amount: number
  filled: number
  total: number
  status: 'open' | 'partial' | 'cancelled'
}

export interface WalletState {
  address: string
  lbtc_sats: number
  assets: Record<string, number>
}

export interface ApiState {
  network?: string
  block_count: number
  assets: string[]
  maker: WalletState
  taker: WalletState
  offers: Offer[]
}

export interface Quote {
  quote_id: number
  from_ticker: string
  to_ticker: string
  from_amount: number
  to_amount: number
  rate_text: string
  fee_text: string
  expires_at: number
}

export interface SwapRecord {
  from_ticker: string
  to_ticker: string
  from_amount: number
  to_amount: number
  txid: string
  timestamp: number
  status: string
}
