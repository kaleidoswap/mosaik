import type { AssetTrust } from './assetTrust';
import { Notice } from '../ui';

/** What the index and the other orders say about the asset name a maker used. Shown only when there is something to say. */
export function TrustNotice({ trust, assetId }: { trust: AssetTrust; assetId: string }) {
  if (trust.level === 'listed') return null;
  const tone = trust.level === 'warning' ? 'warning' : trust.level === 'caution' ? 'warning' : 'info';
  return (
    <Notice tone={tone}>
      {trust.notes.length === 1 ? (
        <p>{trust.notes[0]}</p>
      ) : (
        <ul className="list-disc pl-4">
          {trust.notes.map((n) => <li key={n}>{n}</li>)}
        </ul>
      )}
      <p className="mt-1 break-all font-mono text-tiny opacity-80">Asset id {assetId}. The id is the asset; a name is only what the maker calls it.</p>
    </Notice>
  );
}
