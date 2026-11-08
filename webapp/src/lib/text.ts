/**
 * Text that comes from a maker or a relay is shown, never trusted. These keep it to one readable
 * line (no control, direction-override or zero-width characters that make one string look like
 * another) and give a form to compare tickers by.
 */

// controls, bidi embeddings/overrides/isolates, zero width and joiners, BOM, line/paragraph separators
const HIDDEN_RANGES: Array<[number, number]> = [
  [0x0000, 0x001f], [0x007f, 0x009f], [0x00ad, 0x00ad], [0x061c, 0x061c], [0x180e, 0x180e],
  [0x200b, 0x200f], [0x2028, 0x202e], [0x2060, 0x206f], [0xfeff, 0xfeff], [0xfff9, 0xfffb],
];
const HIDDEN = new RegExp(
  '[' + HIDDEN_RANGES.map(([a, b]) => String.fromCharCode(a) + '-' + String.fromCharCode(b)).join('') + ']',
  'g',
);

export function safeLabel(s: unknown, max: number): string | undefined {
  if (typeof s !== 'string') return undefined;
  const out = s.normalize('NFKC').replace(HIDDEN, '').replace(/\s+/g, ' ').trim().slice(0, max);
  return out || undefined;
}

// letters that read as a latin one; enough to catch the cheap copies of a ticker
const LOOKALIKE: Record<string, string> = {
  а: 'a', в: 'b', с: 'c', е: 'e', н: 'h', к: 'k', м: 'm', о: 'o', р: 'p', т: 't', х: 'x', у: 'y', і: 'i', ј: 'j', ѕ: 's',
  α: 'a', β: 'b', ε: 'e', ι: 'i', κ: 'k', ν: 'v', ο: 'o', ρ: 'p', τ: 't', υ: 'u', χ: 'x',
  '0': 'o', '1': 'l', '5': 's', '|': 'l',
};

/** Two tickers with the same skeleton look alike to a person: `USDT`, `U5DT`, `USDТ` (cyrillic T). */
export function tickerSkeleton(t: string): string {
  return [...t.normalize('NFKC').toLowerCase().replace(HIDDEN, '')]
    .filter((c) => /[\p{L}\p{N}|]/u.test(c))
    .map((c) => LOOKALIKE[c] ?? c)
    .join('');
}

/**
 * A ticker with any letter outside ASCII. The hand-made look-alike table above cannot cover every
 * confusable (UTS #39 lists thousands), so a non-ASCII ticker is treated as suspect outright.
 */
export function mixesScripts(t: string): boolean {
  return /[^\x00-\x7f]/.test(t);
}
