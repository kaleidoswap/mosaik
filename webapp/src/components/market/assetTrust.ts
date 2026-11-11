/**
 * How far to trust the name on an order. The asset id is the only identity; a ticker is a
 * claim. This sets the maker's claim beside the index and beside the other orders on the page,
 * and says when it looks like a copy.
 */
import type { RgbOffer } from '../../api/rgbp2p';
import type { Listed } from '../../api/registry';
import { mixesScripts, tickerSkeleton } from '../../lib/text';

export type TrustLevel = 'unknown' | 'listed' | 'unlisted' | 'caution' | 'warning';

export interface AssetTrust {
  level: TrustLevel;
  /** short lines, worst first */
  notes: string[];
  /** The ticker the index lists the asset under, when it differs from the maker's. */
  indexTicker?: string;
  /** Other assets that read the same as this one's ticker. */
  lookalikes: string[];
}

export function assetTrust(offer: Pick<RgbOffer, 'assetId' | 'ticker'>, offers: Pick<RgbOffer, 'assetId' | 'ticker'>[], list: Listed[] | undefined): AssetTrust {
  const notes: string[] = [];
  const listed = list?.find((l) => l.contractId === offer.assetId);
  const claimed = offer.ticker;
  let level: TrustLevel = listed ? 'listed' : list === undefined ? 'unknown' : 'unlisted';
  let indexTicker: string | undefined;

  if (listed && claimed && listed.ticker && tickerSkeleton(listed.ticker) !== tickerSkeleton(claimed)) {
    indexTicker = listed.ticker;
    notes.push(`The maker calls it ${claimed}; the index lists it as ${listed.ticker}.`);
    level = 'warning';
  }

  const lookalikes = new Set<string>();
  if (claimed) {
    const sk = tickerSkeleton(claimed);
    for (const o of offers) if (o.assetId !== offer.assetId && o.ticker && tickerSkeleton(o.ticker) === sk) lookalikes.add(o.assetId);
    for (const l of list ?? []) if (l.contractId !== offer.assetId && l.ticker && tickerSkeleton(l.ticker) === sk) lookalikes.add(l.contractId);
    if (lookalikes.size > 0) {
      notes.push(`${lookalikes.size} other asset${lookalikes.size > 1 ? 's' : ''} use the same or a look-alike ticker. Check the asset id.`);
      if (level !== 'warning') level = 'caution';
    }
    if (mixesScripts(claimed)) {
      notes.push('The ticker mixes alphabets, which is how copies are made.');
      level = 'warning';
    }
  }
  if (level === 'unknown') notes.push('Registry status unavailable. This does not mean the asset is unlisted.');
  if (!listed && level === 'unlisted') notes.push('Not listed in the RGBMap index.');
  return { level, notes, indexTicker, lookalikes: [...lookalikes] };
}
