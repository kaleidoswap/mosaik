import { useEffect } from 'react'
import { useMarketStore } from './store/useMarketStore'
import Header from './components/Header'
import Orderbook from './components/Orderbook'
import CandlestickChart from './components/CandlestickChart'
import DepthChart from './components/DepthChart'
import TradingPanel from './components/TradingPanel'
import MarketTrades from './components/MarketTrades'
import OpenOrders from './components/OpenOrders'

const POLL_INTERVAL_MS = 3000

export default function App() {
  const refresh = useMarketStore((s) => s.refresh)
  const refreshChart = useMarketStore((s) => s.refreshChart)

  useEffect(() => {
    void refresh()
    void refreshChart()
    const id = setInterval(() => {
      void refresh()
    }, POLL_INTERVAL_MS)
    return () => clearInterval(id)
  }, [refresh, refreshChart])

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden" style={{ background: 'var(--color-bg)' }}>
      <Header />
      <div className="flex-1 grid gap-1 p-1 overflow-hidden"
        style={{
          gridTemplateColumns: '280px 1fr 320px',
          gridTemplateRows: '1fr 1fr 180px',
          gridTemplateAreas: `
            "orderbook chart trades"
            "orderbook depth trades"
            "orderbook orders orders"
          `,
        }}
      >
        <div className="rounded overflow-hidden" style={{ gridArea: 'orderbook', background: 'var(--color-panel)', border: '1px solid var(--color-border)' }}>
          <Orderbook />
        </div>
        <div className="rounded overflow-hidden" style={{ gridArea: 'chart', background: 'var(--color-panel)', border: '1px solid var(--color-border)' }}>
          <CandlestickChart />
        </div>
        <div className="rounded overflow-hidden" style={{ gridArea: 'depth', background: 'var(--color-panel)', border: '1px solid var(--color-border)' }}>
          <DepthChart />
        </div>
        <div className="rounded overflow-hidden flex flex-col" style={{ gridArea: 'trades', background: 'var(--color-panel)', border: '1px solid var(--color-border)' }}>
          <TradingPanel />
          <div className="flex-1 min-h-0 mt-1">
            <MarketTrades />
          </div>
        </div>
        <div className="rounded overflow-hidden" style={{ gridArea: 'orders', background: 'var(--color-panel)', border: '1px solid var(--color-border)' }}>
          <OpenOrders />
        </div>
      </div>
    </div>
  )
}
