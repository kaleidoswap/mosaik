import { Card, CardContent } from 'kaleido-ui';
import type { CatalogAsset } from '../../api/catalog';
import type { Market } from './book';

export function Catalog({ assets, markets, selected, onSelect }: { assets: CatalogAsset[]; markets: Market[]; selected?: string; onSelect: (id: string) => void }) {
  return <section aria-label="Demo asset catalog" className="demo-catalog">
    <div className="catalog-heading"><div><span className="eyebrow">EXPLORE THE TEST MARKET</span><h2>Two assets. Real test trades.</h2></div><span className="demo-badge">No real-world value</span></div>
    <div className="catalog-grid">{assets.map(a => {
      const market = markets.find(m => m.assetId === a.contractId);
      const count = (market?.asks ?? 0) + (market?.bids ?? 0);
      return <button type="button" key={a.contractId} aria-pressed={selected === a.contractId} onClick={() => onSelect(a.contractId)} className={`catalog-choice ${a.ticker === 'TESTXAU' ? 'is-gold' : ''} ${selected === a.contractId ? 'is-selected' : ''}`}>
        <span className="asset-monogram" aria-hidden="true">{a.ticker === 'TESTXAU' ? 'Au' : '$'}</span>
        <span className="catalog-copy"><strong>{a.name}</strong><span>{a.ticker} / BTC · Mutinynet</span><small>{count ? `${market?.bids ?? 0} buy · ${market?.asks ?? 0} sell offers` : 'No active offers · market remains listed'}</small></span><span aria-hidden="true">↗</span>
      </button>;
    })}</div>
  </section>;
}
export function AssetDetails({ asset }: { asset: CatalogAsset }) {
  return <Card className="asset-details"><CardContent><details><summary>About {asset.ticker} <span>Curated demo asset · view identity</span></summary><p>{asset.description}</p><dl><dt>Network</dt><dd>Mutinynet (Signet custom)</dd><dt>Contract ID</dt><dd className="asset-contract">{asset.contractId}</dd><dt>Issued supply</dt><dd>{asset.issuedSupply} base units · {asset.precision} decimals</dd><dt>Metadata source</dt><dd>KaleidoSwap demo catalog. Inclusion is not an issuer attestation.</dd><dt>Pricing</dt><dd>Fixed demo reference with a buy/sell spread. Not a live USD or gold price.</dd></dl><a href="/.well-known/rgb/tokens.json" target="_blank" rel="noopener noreferrer">View versioned asset list ↗</a></details></CardContent></Card>;
}
