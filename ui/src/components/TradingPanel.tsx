import { useState } from 'react'
import { useMarketStore } from '../store/useMarketStore'
import { fetchQuote, confirmQuote, makeOffer } from '../lib/api'

const TABS = ['Limit', 'Market', 'Stop-Limit']
const PERCENTAGES = [25, 50, 75, 100]

export default function TradingPanel() {
  const { pair, state } = useMarketStore()
  const [tab, setTab] = useState('Limit')
  const [side, setSide] = useState<'Buy' | 'Sell'>('Buy')
  const [price, setPrice] = useState('')
  const [amount, setAmount] = useState('')
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<string | null>(null)

  const [base, quoteAsset] = pair.split('-')
  const wallet = state?.maker
  const available = wallet?.assets[quoteAsset] ?? wallet?.lbtc_sats ?? 0
  const availableDisplay = (available / 1e8).toFixed(4)

  const total = price && amount ? (parseFloat(price) * parseFloat(amount)).toFixed(2) : '0.00'

  const handlePercent = (pct: number) => {
    if (!price || parseFloat(price) === 0) return
    const amt = (available / 1e8) * (pct / 100) / parseFloat(price)
    setAmount(amt.toFixed(6))
  }

  const handleSubmit = async () => {
    if (!price || !amount) return
    setLoading(true)
    setMessage(null)
    try {
      if (side === 'Buy') {
        // Taker flow via RFQ
        const q = await fetchQuote(quoteAsset, base, Math.round(parseFloat(amount) * 1e8))
        const result = await confirmQuote(q.quote_id)
        setMessage(`Bought! tx: ${result.txid.slice(0, 16)}...`)
      } else {
        // Maker flow: create offer
        const result = await makeOffer(base, quoteAsset, Math.round(parseFloat(amount) * 1e8), Math.round(parseFloat(price) * 1e8))
        setMessage(`Offer created: ${result.outpoint}`)
      }
    } catch (err: any) {
      setMessage(`Error: ${err.message}`)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="flex flex-col px-3 py-2 shrink-0" style={{ borderBottom: '1px solid var(--color-border)' }}>
      {/* Tabs */}
      <div className="flex border-b mb-2" style={{ borderColor: 'var(--color-border)' }}>
        {TABS.map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className="flex-1 pb-1 text-xs font-medium transition-colors relative"
            style={{
              color: tab === t ? '#fff' : 'var(--color-muted)',
            }}
          >
            {t}
            {tab === t && (
              <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-[var(--color-accent)]" />
            )}
          </button>
        ))}
      </div>

      {/* Buy/Sell toggle */}
      <div className="flex rounded mb-3 overflow-hidden">
        <button
          onClick={() => setSide('Buy')}
          className="flex-1 py-1.5 text-xs font-semibold transition-colors"
          style={{
            background: side === 'Buy' ? 'var(--color-buy)' : 'rgba(38,166,154,0.15)',
            color: side === 'Buy' ? '#fff' : 'var(--color-buy)',
          }}
        >
          Buy
        </button>
        <button
          onClick={() => setSide('Sell')}
          className="flex-1 py-1.5 text-xs font-semibold transition-colors"
          style={{
            background: side === 'Sell' ? 'var(--color-sell)' : 'rgba(239,83,80,0.15)',
            color: side === 'Sell' ? '#fff' : 'var(--color-sell)',
          }}
        >
          Sell
        </button>
      </div>

      {/* Price input */}
      {tab !== 'Market' && (
        <div className="mb-2">
          <div className="flex items-center justify-between mb-1">
            <span className="text-[10px] text-[var(--color-muted)]">Price</span>
          </div>
          <input
            type="number"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            className="w-full px-2 py-1.5 rounded text-xs text-white outline-none"
            style={{ background: '#131722', border: '1px solid var(--color-border)' }}
            placeholder="0.00"
          />
        </div>
      )}

      {/* Amount input */}
      <div className="mb-2">
        <div className="flex items-center justify-between mb-1">
          <span className="text-[10px] text-[var(--color-muted)]">Amount</span>
          <span className="text-[10px] text-white font-medium">{base}</span>
        </div>
        <input
          type="number"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          className="w-full px-2 py-1.5 rounded text-xs text-white outline-none"
          style={{ background: '#131722', border: '1px solid var(--color-border)' }}
          placeholder="0.00"
        />
      </div>

      {/* Quick % buttons */}
      <div className="flex gap-1 mb-2">
        {PERCENTAGES.map((p) => (
          <button
            key={p}
            onClick={() => handlePercent(p)}
            className="flex-1 py-1 rounded text-[10px] font-medium transition-colors hover:bg-white/5"
            style={{ background: '#131722', color: 'var(--color-muted)', border: '1px solid var(--color-border)' }}
          >
            {p}%
          </button>
        ))}
      </div>

      {/* Available */}
      <div className="flex justify-between mb-2 text-[10px]">
        <span className="text-[var(--color-muted)]">Available {quoteAsset}</span>
        <span className="text-white">{availableDisplay}</span>
      </div>

      {/* Total */}
      <div className="flex justify-between mb-3 text-xs">
        <span className="text-[var(--color-muted)]">Total</span>
        <span className="text-white font-medium">{total} {quoteAsset}</span>
      </div>

      {/* Submit */}
      <button
        onClick={() => void handleSubmit()}
        disabled={loading}
        className="w-full py-2 rounded text-xs font-semibold transition-opacity hover:opacity-90 disabled:opacity-50"
        style={{
          background: side === 'Buy' ? 'var(--color-buy)' : 'var(--color-sell)',
          color: '#fff',
        }}
      >
        {loading ? 'Processing...' : `${side} ${base}`}
      </button>

      {message && (
        <div className="mt-2 text-[10px] break-all" style={{ color: message.startsWith('Error') ? 'var(--color-sell)' : 'var(--color-buy)' }}>
          {message}
        </div>
      )}
    </div>
  )
}
