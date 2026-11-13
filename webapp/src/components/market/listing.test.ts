import { describe, expect, it } from 'vitest';
import { parseCatalog, type Catalog } from '../../api/catalog';
import type { MakerScore } from '../../api/signed';
import { isVerifiedMaker, listingOf, makerMark, marketMakers, VERIFIED_THRESHOLDS } from './listing';
import tokens from '../../../public/.well-known/rgb/tokens.json';

const USD = tokens.tokens[0]!.contract_id;
const XAU = tokens.tokens[1]!.contract_id;
const OTHER = 'rgb:ZZZZZZZZ-other-asset';
const M1 = '1'.repeat(64);
const M2 = '2'.repeat(64);

const signed = (raw: object = tokens): Catalog => ({ ...parseCatalog(raw, 'signetcustom'), signed: { pubkey: 'k', eventId: 'e', signedAt: 1 } });
const unsigned = (): Catalog => ({ ...parseCatalog(tokens, 'signetcustom'), signedError: 'no curated list on the relay' });

const score = (maker: string, over: Partial<MakerScore> = {}): MakerScore => ({
  maker, computedAt: 0, expiresAt: 0, settled: 3, volumeSat: 30_000, counterparties: 2, selfTrades: 0, negotiations: 3, aborted: 0,
  abortRate: 0, presence: 0.95, byAsset: { [USD]: { trades: 3, amount: 300, sat: 30_000, counterparties: 2 } }, ...over,
});

describe('listing tiers', () => {
  const registry = [{ contractId: XAU, ticker: 'TESTXAU', name: '', listedBy: [] }];
  it('curated only from a list whose signature checked out', () => {
    expect(listingOf(USD, signed(), registry).tier).toBe('curated');
    expect(listingOf(USD, unsigned(), registry).tier).toBe('unlisted');
  });
  it('registry, unlisted, and unknown when the registry cannot be read', () => {
    expect(listingOf(XAU, unsigned(), registry).tier).toBe('registry');
    expect(listingOf(OTHER, signed(), registry).tier).toBe('unlisted');
    expect(listingOf(OTHER, signed(), undefined).tier).toBe('unknown');
  });
  it('goes by asset id, never by ticker', () => {
    // an asset that claims a curated ticker is still unlisted
    expect(listingOf('rgb:IMPOSTOR-TESTUSD', signed(), []).tier).toBe('unlisted');
  });
});

describe('verified-maker badge', () => {
  it('needs every threshold', () => {
    expect(isVerifiedMaker(score(M1))).toBe(true);
    expect(isVerifiedMaker(score(M1, { settled: 2 }))).toBe(false);
    expect(isVerifiedMaker(score(M1, { counterparties: 1 }))).toBe(false);
    expect(isVerifiedMaker(score(M1, { presence: 0.89 }))).toBe(false);
    expect(isVerifiedMaker(score(M1, { presence: null }))).toBe(false);
    expect(isVerifiedMaker(score(M1, { abortRate: 0.25 }))).toBe(false);
    expect(isVerifiedMaker(undefined)).toBe(false);
  });
  it('follows the configured thresholds', () => {
    expect(isVerifiedMaker(score(M1, { settled: 1, counterparties: 1 }), { ...VERIFIED_THRESHOLDS, minSettled: 1, minCounterparties: 1 })).toBe(true);
  });
});

describe('issuer-is-maker label', () => {
  const withIssuer = { ...tokens, tokens: [{ ...tokens.tokens[0], issuer_makers: [M1] }, tokens.tokens[1]] };
  it('labels the named maker on that asset only', () => {
    expect(makerMark(M1, USD, signed(withIssuer), undefined).issuer).toBe(true);
    expect(makerMark(M2, USD, signed(withIssuer), undefined).issuer).toBe(false);
    expect(makerMark(M1, XAU, signed(withIssuer), undefined).issuer).toBe(false);
  });
  it('never from an unsigned list', () => {
    const c: Catalog = { ...parseCatalog(withIssuer, 'signetcustom') };
    expect(makerMark(M1, USD, c, undefined).issuer).toBe(false);
  });
});

describe('makers of a market', () => {
  it('lists open makers and makers with volume, and sums volume, not counterparties', () => {
    const scores = new Map([[M1, score(M1)], [M2, score(M2, { byAsset: { [USD]: { trades: 1, amount: 100, sat: 5_000, counterparties: 1 } } })]]);
    const r = marketMakers(USD, [M2, M2, '3'.repeat(64)], signed(), scores);
    expect(r.rows.map((x) => [x.maker[0], x.open, x.trades, x.sat, x.counterparties])).toEqual([
      ['1', 0, 3, 30_000, 2],
      ['2', 2, 1, 5_000, 1],
      ['3', 1, 0, 0, undefined],
    ]);
    expect(r.sat).toBe(35_000);
    expect(r.trades).toBe(4);
    expect(r.rows[0]!.mark.verified).toBe(true);
  });
});
