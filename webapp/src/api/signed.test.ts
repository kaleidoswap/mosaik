import { describe, expect, it } from 'vitest';
import { finalizeEvent, generateSecretKey, getPublicKey, type Event } from 'nostr-tools';
import { checkCatalogEvent, checkReputationEvent, newestList, scoresFrom } from './signed';
import { parseCatalog } from './catalog';
import tokens from '../../public/.well-known/rgb/tokens.json';

const sk = generateSecretKey();
const KEY = getPublicKey(sk);
const other = generateSecretKey();
const NOW = 1_800_000_000;
const MAKER = 'a'.repeat(64);
const ASSET = tokens.tokens[0]!.contract_id;

/** A list event shaped exactly as services/mosaik-indexer signs it. */
function listEvent(catalog: Record<string, unknown> = tokens, key = sk, at = NOW): Event {
  const toks = catalog.tokens as { contract_id: string; ticker: string; issuer_makers?: string[] }[];
  return finalizeEvent({
    kind: 30055,
    created_at: at,
    content: JSON.stringify(catalog),
    tags: [
      ['d', 'mosaik-catalog:signetcustom'],
      ['t', 'mosaik-catalog-v1'],
      ['network', 'signetcustom'],
      ['version', String(catalog.version)],
      ...toks.flatMap((t) => [['asset', t.contract_id, t.ticker], ...(t.issuer_makers ?? []).map((p) => ['issuer_maker', t.contract_id, p])]),
    ],
  }, key);
}

function scoreEvent(over: { settled?: number; counterparties?: number; presence?: string; abortRate?: string; at?: number; key?: Uint8Array } = {}): Event {
  const settled = over.settled ?? 4;
  const content = {
    v: 1, network: 'signetcustom', computedAt: over.at ?? NOW, maker: MAKER,
    volume: { sat: 50_000, trades: settled, counterparties: over.counterparties ?? 3, selfTrades: 1, unattributedTrades: 0,
      byAsset: { [ASSET]: { trades: settled, amount: 400, sat: 50_000, counterparties: 2 } } },
  };
  return finalizeEvent({
    kind: 30054,
    created_at: over.at ?? NOW,
    content: JSON.stringify(content),
    tags: [
      ['d', MAKER], ['p', MAKER], ['t', 'mosaik-reputation-v1'], ['network', 'signetcustom'], ['window', '2592000'],
      ['settled', String(settled)], ['volume_sat', '50000'], ['counterparties', String(over.counterparties ?? 3)], ['self_trades', '1'],
      ['negotiations', '5'], ['aborted', '1'], ['verified', '1'], ['expiration', String((over.at ?? NOW) + 172_800)],
      ['presence', over.presence ?? '0.9500'], ['abort_rate', over.abortRate ?? '0.2000'],
    ],
  }, over.key ?? sk);
}

describe('curated list event', () => {
  it('passes with the right key and parses into the catalog', () => {
    const r = checkCatalogEvent(listEvent(), KEY, 'signetcustom');
    expect(r.ok).toBe(true);
    if (r.ok) expect(parseCatalog(r.value.content, 'signetcustom').assets.map((a) => a.ticker)).toEqual(['TESTUSD', 'TESTXAU']);
  });
  it('carries issuer makers through to the catalog', () => {
    const withIssuer = { ...tokens, tokens: [{ ...tokens.tokens[0], issuer_makers: [MAKER] }, tokens.tokens[1]] };
    const r = checkCatalogEvent(listEvent(withIssuer), KEY, 'signetcustom');
    expect(r.ok && parseCatalog(r.value.content, 'signetcustom').assets[0]!.issuerMakers).toEqual([MAKER]);
  });
  it('refuses another key, a forged signature, another network and tags that lie', () => {
    expect(checkCatalogEvent(listEvent(tokens, other), KEY, 'signetcustom').ok).toBe(false);
    const e = listEvent();
    expect(checkCatalogEvent({ ...e, content: e.content.replace('TESTUSD', 'TESTUSDT') }, KEY, 'signetcustom').ok).toBe(false);
    expect(checkCatalogEvent(e, KEY, 'bitcoin').ok).toBe(false);
    const lying = finalizeEvent({ ...e, tags: e.tags.filter((t) => !(t[0] === 'asset' && t[2] === 'TESTXAU')) }, sk);
    expect(checkCatalogEvent(lying, KEY, 'signetcustom')).toEqual({ ok: false, error: 'asset tags do not match the list' });
  });
  it('takes the newest valid list and ignores the rest', () => {
    const older = listEvent(tokens, sk, NOW - 100);
    const newer = listEvent({ ...tokens, version: '0.2.0' }, sk, NOW);
    const forged = listEvent({ ...tokens, version: '9.9.9' }, other, NOW + 100);
    const r = newestList([older, forged, newer], KEY, 'signetcustom');
    expect(r.ok && r.value.eventId).toBe(newer.id);
    expect(newestList([forged], KEY, 'signetcustom').ok).toBe(false);
  });
});

describe('reputation event', () => {
  it('reads the numbers from a signed score', () => {
    const r = checkReputationEvent(scoreEvent(), KEY, 'signetcustom', NOW + 60);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value).toMatchObject({ maker: MAKER, settled: 4, counterparties: 3, volumeSat: 50_000, presence: 0.95, abortRate: 0.2 });
      expect(r.value.byAsset[ASSET]).toEqual({ trades: 4, amount: 400, sat: 50_000, counterparties: 2 });
    }
  });
  it('refuses another key, an expired score, and tags that disagree with the content', () => {
    expect(checkReputationEvent(scoreEvent({ key: other }), KEY, 'signetcustom', NOW).ok).toBe(false);
    expect(checkReputationEvent(scoreEvent(), KEY, 'signetcustom', NOW + 172_801).ok).toBe(false);
    const e = scoreEvent();
    const inflated = finalizeEvent({ ...e, tags: e.tags.map((t) => (t[0] === 'settled' ? ['settled', '40'] : t)) }, sk);
    expect(checkReputationEvent(inflated, KEY, 'signetcustom', NOW).ok).toBe(false);
  });
  it('keeps the newest valid score per maker', () => {
    const scores = scoresFrom([scoreEvent({ settled: 2, at: NOW - 600 }), scoreEvent({ settled: 5 }), scoreEvent({ settled: 9, key: other })], KEY, 'signetcustom', NOW);
    expect(scores.size).toBe(1);
    expect(scores.get(MAKER)?.settled).toBe(5);
  });
});

