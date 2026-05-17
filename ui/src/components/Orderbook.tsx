import { useMarketStore } from '../store/useMarketStore'
import type { OrderbookLevel } from '../types'

function LevelRow({ level, side, maxTotal }: { level: OrderbookLevel; side: 'bid' | 'ask'; maxTotal: number }) {
  const color = side === 'bid' ? 'var(--color-buy)' : 'var(--color-sell)'
  const bg = side === 'bid' ? 'var(--color-buy-bg)' : 'var(--color-sell-bg)'
  const pct = maxTotal > 0 ? (level.total / maxTotal) * 100 : 0

  return (
    <div className="relative flex items-center justify-between px-2 py-0.5 text-[11px] cursor-pointer hover:bg-white/5"
      onClick={() => {
        // Could set trading panel price
      }}
    >
      <div
        className="absolute top-0 bottom-0"
        style={{
          [side === 'bid' ? 'left' : 'right']: 0,
          width: `${pct}%`,
          background: bg,
        }}
      />
      <span className="relative z-10 font-mono" style={{ color }}>{level.price.toFixed(2)}</span>
      <span className="relative z-10 text-white/80">{level.amount.toFixed(4)}</span>
      <span className="relative z-10 text-white/60">{level.total.toFixed(4)}</span>
    </div>
  )
}

export default function Orderbook() {
  const { orderbook } = useMarketStore()
  // Asks sorted ascending by API — lowest ask first. Show at top.
  const asks = orderbook.asks.slice(0, 8)
  // Bids sorted descending by API — highest bid first. Show below spread.
  const bids = orderbook.bids.slice(0, 8)
  const all = [...asks, ...bids]
  const maxTotal = all.length > 0 ? Math.max(...all.map((l) => l.total)) : 1

  const bestAsk = orderbook.asks[0]?.price ?? 0
  const bestBid = orderbook.bids[0]?.price ?? 0
  const spread = bestAsk > 0 && bestBid > 0 ? bestAsk - bestBid : 0

  return (
    <div className="h-full flex flex-col">
      <div className="px-3 py-2 text-xs font-semibold text-white border-b" style={{ borderColor: 'var(--color-border)' }}>
        Orderbook
      </div>
      <div className="flex text-[10px] text-[var(--color-muted)] px-2 py-1 border-b" style={{ borderColor: 'var(--color-border)' }}>
        <span className="flex-1">Price</span>
        <span className="flex-1 text-right">Amount</span>
        <span className="flex-1 text-right">Total</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {/* Asks (red) - lowest at top */}
        {asks.map((level, i) => (
          <LevelRow key={`ask-${i}`} level={level} side="ask" maxTotal={maxTotal} />
        ))}
        {/* Spread */}
        <div className="flex items-center justify-center py-1 text-[11px] text-[var(--color-muted)] border-y" style={{ borderColor: 'var(--color-border)' }}>
          Spread: {spread.toFixed(2)}
        </div>
        {/* Bids (green) - highest at top (closest to spread) */}
        {bids.map((level, i) => (
          <LevelRow key={`bid-${i}`} level={level} side="bid" maxTotal={maxTotal} />
        ))}
      </div>
    </div>
  )
}
