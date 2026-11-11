import { useWebRgb } from '../api/webrgb';
import { fmtSats, shortId } from '../lib/format';

const units = (n: number, precision: number) => (n / 10 ** precision).toLocaleString(undefined, { maximumFractionDigits: precision });

/** What the connected wallet holds, read from the extension. Nothing here can move funds. */
export function WalletPanel() {
  const w = useWebRgb();
  if (w.status !== 'connected') {
    return (
      <section className="panel p-8 text-center">
        <h2 className="text-lg font-semibold">Sign in with your wallet</h2>
        <p className="mx-auto mt-2 max-w-md text-sm text-content-secondary">
          Connect the KaleidoSwap extension to see your balances and take orders. The page only reads what you allow; the extension keeps your keys and asks you before every swap.
        </p>
        <button className="btn-primary mt-5" disabled={w.status === 'absent' || w.status === 'connecting'} onClick={() => void w.connect()} type="button">
          {w.status === 'absent' ? 'Extension not found' : w.status === 'connecting' ? 'Approve in the wallet…' : 'Connect wallet'}
        </button>
        {w.error && <p className="mt-3 text-sm text-status-danger">{w.error}</p>}
      </section>
    );
  }
  return (
    <div className="grid gap-4 md:grid-cols-3">
      <section className="panel p-4">
        <div className="panel-title">Bitcoin</div>
        <div className="mt-2 text-2xl font-semibold">{fmtSats(w.balance?.spendable ?? 0)} <span className="text-sm text-content-secondary">sat</span></div>
        <div className="mt-1 text-xs text-content-tertiary">available for swaps · network {w.walletLabel ?? w.network ?? 'unknown'}</div>
        {w.balance?.rgbReserved !== undefined && <p className="mt-2 text-xs text-content-secondary">{fmtSats(w.balance.rgbReserved)} sat held in RGB outputs. Empty reusable outputs may also be included above; do not add these figures.</p>}
        {w.balance?.freeColorableUtxos === 0 && (
          <p className="mt-3 rounded-lg bg-status-warning-subtle p-2 text-xs text-status-warning">
            No free coin for RGB. Create one in the extension before taking an order.
          </p>
        )}
      </section>
      <section className="panel md:col-span-2">
        <header className="panel-header"><h2 className="panel-title">RGB assets</h2><span className="text-[10px] font-mono text-content-tertiary">{w.assets.length}</span></header>
        {w.assets.length === 0 ? (
          <div className="p-6 text-center text-sm text-content-tertiary">No RGB assets in this wallet yet.</div>
        ) : (
          <ul className="divide-y divide-border-subtle">
            {w.assets.map((a) => (
              <li key={a.id} className="flex items-baseline gap-3 px-4 py-3">
                <span className="font-medium">{a.ticker}</span>
                <span className="text-sm text-content-secondary">{a.name}</span>
                <span className="ml-auto font-mono text-sm">{units(a.balance, a.precision)}</span>
                <span className="font-mono text-[10px] text-content-tertiary" title={a.id}>{shortId(a.id.replace('rgb:', ''), 6, 4)}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
