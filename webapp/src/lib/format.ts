/** Display helpers — sats, asset names, shortened ids. */

const SATS_PER_BTC = 100_000_000;

export function fmtSats(sats: number): string {
  if (sats === 0) return '0';
  if (sats < 1_000) return sats.toString();
  return sats.toLocaleString();
}

export function fmtBtc(sats: number): string {
  const btc = sats / SATS_PER_BTC;
  // Use up to 8 dp but trim trailing zeroes for readability.
  return btc.toFixed(8).replace(/\.?0+$/, '');
}

export function fmtRate(amountA: number, amountB: number): string {
  if (amountA <= 0) return '—';
  const r = amountB / amountA;
  if (r >= 1) return r.toFixed(4);
  if (r >= 0.01) return r.toFixed(6);
  return r.toExponential(2);
}

export function shortId(id: string, head = 6, tail = 4): string {
  if (id.length <= head + tail + 1) return id;
  return `${id.slice(0, head)}…${id.slice(-tail)}`;
}

export function pairLabel(lock: string, want: string): string {
  return `${lock}/${want}`;
}
