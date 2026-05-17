import { useMarketStore } from '../store/useMarketStore'

export default function MarketTrades() {
  const { trades } = useMarketStore()

  return (
    <div className="h-full flex flex-col">
      <div className="px-3 py-2 text-xs font-semibold text-white border-b" style={{ borderColor: 'var(--color-border)' }}>
        Market Trades
      </div>
      <div className="flex text-[10px] text-[var(--color-muted)] px-2 py-1 border-b" style={{ borderColor: 'var(--color-border)' }}>
        <span className="flex-1">Price</span>
        <span className="flex-1 text-right">Amount</span>
        <span className="flex-1 text-right">Time</span>
      </div>
      <div className="flex-1 overflow-y-auto">
        {trades.length === 0 && (
          <div className="px-2 py-4 text-[10px] text-center text-[var(--color-muted)]">No trades yet</div>
        )}
        {trades.map((trade, i) => (
          <div key={i} className="flex items-center px-2 py-0.5 text-[11px]">
            <span
              className="flex-1 font-mono"
              style={{ color: trade.side === 'buy' ? 'var(--color-buy)' : 'var(--color-sell)' }}
            >
              {trade.price.toFixed(2)}
            </span>
            <span className="flex-1 text-right text-white/80">{trade.amount.toFixed(4)}</span>
            <span className="flex-1 text-right text-white/60">{trade.time}</span>
          </div>
        ))}
      </div>
    </div>
  )
}
