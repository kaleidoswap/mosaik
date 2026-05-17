import { useState } from 'react'
import { useMarketStore } from '../store/useMarketStore'
import { cancelOrder } from '../lib/api'

export default function OpenOrders() {
  const { openOrders, refresh } = useMarketStore()
  const [cancelling, setCancelling] = useState<number | null>(null)

  const handleCancel = async (id: number) => {
    setCancelling(id)
    try {
      await cancelOrder(id)
      await refresh()
    } catch (err: any) {
      alert(err.message)
    } finally {
      setCancelling(null)
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center justify-between px-3 py-2 border-b" style={{ borderColor: 'var(--color-border)' }}>
        <span className="text-xs font-semibold text-white">Open Orders</span>
        <button
          onClick={() => void refresh()}
          className="text-[10px] text-[var(--color-accent)] hover:underline"
        >
          Refresh
        </button>
      </div>
      <div className="flex-1 overflow-auto">
        <table className="w-full text-[11px]">
          <thead>
            <tr className="text-[var(--color-muted)] border-b" style={{ borderColor: 'var(--color-border)' }}>
              <th className="text-left px-2 py-1 font-medium">Type</th>
              <th className="text-left px-2 py-1 font-medium">Pair</th>
              <th className="text-right px-2 py-1 font-medium">Price</th>
              <th className="text-right px-2 py-1 font-medium">Amount</th>
              <th className="text-right px-2 py-1 font-medium">Total</th>
              <th className="text-right px-2 py-1 font-medium">Status</th>
              <th className="text-right px-2 py-1 font-medium">Action</th>
            </tr>
          </thead>
          <tbody>
            {openOrders.length === 0 && (
              <tr>
                <td colSpan={7} className="px-2 py-4 text-center text-[var(--color-muted)]">
                  No open orders
                </td>
              </tr>
            )}
            {openOrders.map((order) => (
              <tr key={order.id} className="border-b hover:bg-white/5" style={{ borderColor: 'var(--color-border)' }}>
                <td className="px-2 py-1">
                  <span
                    className="px-1.5 py-0.5 rounded text-[10px] font-semibold"
                    style={{
                      background: order.type === 'BUY' ? 'var(--color-buy-bg)' : 'var(--color-sell-bg)',
                      color: order.type === 'BUY' ? 'var(--color-buy)' : 'var(--color-sell)',
                    }}
                  >
                    {order.type}
                  </span>
                </td>
                <td className="px-2 py-1 text-white">{order.pair}</td>
                <td className="px-2 py-1 text-right text-white font-mono">{order.price.toFixed(2)}</td>
                <td className="px-2 py-1 text-right text-white/80">{order.amount.toFixed(4)}</td>
                <td className="px-2 py-1 text-right text-white/80">{order.total.toFixed(2)}</td>
                <td className="px-2 py-1 text-right text-[var(--color-muted)] capitalize">{order.status}</td>
                <td className="px-2 py-1 text-right">
                  <button
                    onClick={() => void handleCancel(order.id)}
                    disabled={cancelling === order.id}
                    className="px-2 py-0.5 rounded text-[10px] transition-colors hover:bg-white/10 disabled:opacity-50"
                    style={{ color: 'var(--color-sell)', border: '1px solid var(--color-sell)' }}
                  >
                    {cancelling === order.id ? '...' : 'Cancel'}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
