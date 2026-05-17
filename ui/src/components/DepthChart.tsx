import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts'
import { useMarketStore } from '../store/useMarketStore'

export default function DepthChart() {
  const { depth } = useMarketStore()
  const data = depth

  return (
    <div className="h-full flex flex-col">
      <div className="px-3 py-2 text-xs font-semibold text-white border-b" style={{ borderColor: 'var(--color-border)' }}>
        Depth Chart
      </div>
      <div className="flex-1 min-h-0 p-2">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data} margin={{ top: 5, right: 5, left: 5, bottom: 5 }}>
            <XAxis
              dataKey="price"
              type="number"
              domain={['dataMin', 'dataMax']}
              tick={{ fill: '#787b86', fontSize: 10 }}
              axisLine={{ stroke: '#2a2e39' }}
              tickLine={false}
            />
            <YAxis
              tick={{ fill: '#787b86', fontSize: 10 }}
              axisLine={false}
              tickLine={false}
              width={40}
            />
            <Tooltip
              contentStyle={{ background: '#1e222d', border: '1px solid #2a2e39', borderRadius: 4, fontSize: 12 }}
              itemStyle={{ color: '#d1d4dc' }}
              labelStyle={{ color: '#787b86' }}
            />
            <Area
              type="step"
              dataKey="bidDepth"
              stroke="#26a69a"
              fill="rgba(38,166,154,0.2)"
              strokeWidth={1}
            />
            <Area
              type="step"
              dataKey="askDepth"
              stroke="#ef5350"
              fill="rgba(239,83,80,0.2)"
              strokeWidth={1}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}
