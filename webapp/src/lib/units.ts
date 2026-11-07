/** Amounts and times as the market shows them. Asset amounts arrive in the smallest unit. */

export const fmtUnits = (amount: number, precision: number): string =>
  (amount / 10 ** precision).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: precision });

export const fmtPrice = (satPerUnit: number): string =>
  satPerUnit.toLocaleString(undefined, { maximumFractionDigits: satPerUnit < 10 ? 2 : satPerUnit < 1000 ? 1 : 0 });

/** Time left until a unix-seconds expiry, coarse: `3d`, `5h`, `12m`, `45s`. */
export const fmtLeft = (expiry: number, now: number): string => {
  const s = Math.max(0, expiry - now);
  if (s >= 86_400) return `${Math.floor(s / 86_400)}d ${Math.floor((s % 86_400) / 3_600)}h`;
  if (s >= 3_600) return `${Math.floor(s / 3_600)}h ${Math.floor((s % 3_600) / 60)}m`;
  if (s >= 60) return `${Math.floor(s / 60)}m`;
  return `${s}s`;
};

/** `m:ss` for a number of seconds. */
export const fmtClock = (seconds: number): string => {
  const s = Math.max(0, Math.floor(seconds));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

export const EXPLORER_TX = 'https://mutinynet.com/tx/';
