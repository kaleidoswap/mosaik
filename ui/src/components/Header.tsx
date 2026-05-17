import { useMarketStore } from '../store/useMarketStore'
import { fundMaker, fundTaker } from '../lib/api'

export default function Header() {
  const { pair, price, prevPrice, state, refresh } = useMarketStore()

  const priceChange = prevPrice > 0 ? ((price - prevPrice) / prevPrice) * 100 : 0
  const isUp = priceChange >= 0
  const displayPrice = price > 0 ? price.toFixed(2) : '—'
  const displayChange = `${isUp ? '+' : ''}${priceChange.toFixed(2)}%`

  const high = (price * 1.02).toFixed(2)
  const low = (price * 0.98).toFixed(2)

  const handleFund = async () => {
    try {
      await fundMaker()
      await fundTaker()
      await refresh()
    } catch (err: any) {
      alert(err.message)
    }
  }

  return (
    <header
      className="flex items-center justify-between px-4 h-12 shrink-0"
      style={{ background: 'var(--color-panel)', borderBottom: '1px solid var(--color-border)' }}
    >
      <div className="flex items-center gap-6">
        {/* Pair selector */}
        <div className="flex items-center gap-2">
          <span className="text-sm font-bold text-white">{pair.replace('-', '/')}</span>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" className="text-[var(--color-muted)]">
            <path d="M3 5L6 8L9 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </div>

        {/* Price */}
        <div className="flex items-baseline gap-2">
          <span className="text-lg font-bold" style={{ color: isUp ? 'var(--color-buy)' : 'var(--color-sell)' }}>
            {displayPrice}
          </span>
          <span className="text-xs" style={{ color: isUp ? 'var(--color-buy)' : 'var(--color-sell)' }}>
            {displayChange}
          </span>
        </div>

        {/* Stats */}
        <div className="flex items-center gap-4 text-xs" style={{ color: 'var(--color-muted)' }}>
          <span>24h High: <span className="text-white">{high}</span></span>
          <span>24h Low: <span className="text-white">{low}</span></span>
          <span>Block: <span className="text-white">{state?.block_count ?? '—'}</span></span>
        </div>
      </div>

      <div className="flex items-center gap-3">
        <button
          onClick={() => void handleFund()}
          className="px-3 py-1 rounded text-[10px] font-semibold transition-colors hover:opacity-80"
          style={{ background: 'var(--color-accent)', color: '#fff' }}
        >
          Fund Wallets
        </button>
        <div
          className="flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[10px] font-semibold"
          style={{
            background: state?.network === 'testnet' ? 'rgba(41, 98, 255, 0.1)' : 'rgba(43, 238, 121, 0.1)',
            color: state?.network === 'testnet' ? 'var(--color-accent)' : 'var(--color-buy)',
            border: state?.network === 'testnet' ? '1px solid rgba(41, 98, 255, 0.2)' : '1px solid rgba(43, 238, 121, 0.2)',
          }}
        >
          <span className="text-[10px]">⬡</span>
          {state?.network ?? 'unknown'}
        </div>
      </div>
    </header>
  )
}
