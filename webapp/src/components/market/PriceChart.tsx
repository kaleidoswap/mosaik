import { useState } from 'react';
import { fmtPrice } from '../../lib/units';
import { candlesOf, type Point } from './stats';

const W = 720, H = 300, LEFT = 12, RIGHT = 90, TOP = 22, BOTTOM = 34;
export function PriceChart({ points, ticker }: { points: Point[]; ticker: string }) {
  const [mode, setMode] = useState<'candles' | 'line'>('candles');
  const [interval, setInterval] = useState(3600);
  const candles = candlesOf(points, interval);
  const values = candles.flatMap(c => [c.high,c.low]);
  const low = values.length ? Math.min(...values) : 0, high = values.length ? Math.max(...values) : 1;
  const span = high-low || Math.max(high*.02,1);
  const t0 = candles[0]?.at ?? 0, t1 = Math.max(candles.at(-1)?.at ?? 0, points.at(-1)?.at ?? 0,t0+interval);
  const x = (t:number) => LEFT+12+(t-t0)/(t1-t0)*(W-LEFT-RIGHT-24);
  const y = (v:number) => TOP+(1-(v-low+span*.1)/(span*1.2))*(H-TOP-BOTTOM);
  const width = Math.max(2,Math.min(30,(W-LEFT-RIGHT-24)*interval/(t1-t0)*.6));
  const time = (at:number) => new Date(at*1000).toLocaleString(undefined,{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'});
  return <section aria-label="Price chart" className="trade-chart">
    <header><div><h2>{ticker} / BTC</h2><span>sat per {ticker}</span></div><div className="chart-controls"><div role="group" aria-label="Chart style" className="view-switch">{(['candles','line'] as const).map(v=><button key={v} aria-pressed={mode===v} onClick={()=>setMode(v)}>{v==='candles'?'Candles':'Line'}</button>)}</div>{mode==='candles' && <select aria-label="Candle interval" value={interval} onChange={e=>setInterval(Number(e.target.value))}><option value={900}>15m</option><option value={3600}>1h</option><option value={14400}>4h</option></select>}</div></header>
    {!candles.length ? <div className="chart-empty"><span aria-hidden="true">⌁</span><h3>No trades reported yet</h3><p>The chart appears when makers report trades. Open orders alone do not create candles.</p></div> : <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${mode==='candles'?'Candlestick':'Price line'} chart for ${ticker}: ${points.length} reported trades`}>
      {[high+span*.05,(high+low)/2,low-span*.05].map((v,i)=><g key={i}><line x1={LEFT} x2={W-RIGHT} y1={y(v)} y2={y(v)} stroke="currentColor" strokeOpacity=".12" strokeDasharray="3 4"/><text x={W-RIGHT+8} y={y(v)+4} fill="currentColor" fontSize="11">{fmtPrice(v)}</text></g>)}
      {mode==='candles' ? candles.map(c=><g key={c.at} stroke={c.close>=c.open?'#61dfad':'#f7808b'} fill={c.close>=c.open?'#61dfad':'#f7808b'}><title>{`${time(c.at)} · O ${fmtPrice(c.open)} H ${fmtPrice(c.high)} L ${fmtPrice(c.low)} C ${fmtPrice(c.close)} · ${c.trades} reported trades`}</title><line x1={x(c.at)} x2={x(c.at)} y1={y(c.high)} y2={y(c.low)} strokeWidth="1.5"/><rect x={x(c.at)-width/2} y={Math.min(y(c.open),y(c.close))-1} width={width} height={Math.max(2,Math.abs(y(c.open)-y(c.close)))} rx="1"/></g>) : <><path d={points.map((p,i)=>`${i?'L':'M'}${x(p.at)},${y(p.price)}`).join(' ')} fill="none" stroke="#61dfad" strokeWidth="2"/>{points.map((p,i)=><circle key={i} cx={x(p.at)} cy={y(p.price)} r="3" fill="#61dfad"><title>{`${time(p.at)} · ${fmtPrice(p.price)} sat`}</title></circle>)}</>}
      <text x={LEFT} y={H-8} fill="currentColor" fontSize="10">{time(t0)}</text><text x={W-RIGHT} y={H-8} textAnchor="end" fill="currentColor" fontSize="10">{time(t1)}</text>
    </svg>}
    <p className="chart-source">Maker-reported trades · not independently verified. Limited history (up to 10 recent trades per maker); empty intervals stay empty.</p>
  </section>;
}
