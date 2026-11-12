import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { generateSecretKey } from 'nostr-tools/pure';
import { bytesToHex, signerFromHex } from '../src/nostr.ts';
import {
  CATALOG_KIND,
  REPUTATION_KIND,
  REPUTATION_TTL_SECS,
  catalogTemplate,
  reputationTemplate,
  verifyCatalogEvent,
  verifyReputationEvent,
} from '../src/events.ts';
import { parseCatalog } from '../src/catalog.ts';
import { DEFAULT_THRESHOLDS, type MakerScore } from '../src/scoring.ts';

const signer = signerFromHex(bytesToHex(generateSecretKey()));
const other = signerFromHex(bytesToHex(generateSecretKey()));
const NOW = 1_800_000_000;
const MAKER = '2e5edeb4f4dcadf9b14bdf0150eebb70b882c57ba994f216dc5dad7bbe0cb43b';

const score: MakerScore = {
  maker: MAKER,
  window: { from: NOW - 30 * 86_400, to: NOW },
  firstSeen: NOW - 5 * 86_400,
  presence: { ratio: 0.9731, presentBuckets: 1401, observedBuckets: 1440, bucketSecs: 300 },
  negotiations: { taken: 6, settled: 4, aborted: 1, unsettled: 0, pending: 1, abortRate: 0.2 },
  volume: { sat: 120_000, trades: 3, counterparties: 2, selfTrades: 1, unattributedTrades: 0, byAsset: { 'rgb:x': { trades: 3, amount: 300, sat: 120_000 } } },
  verified: true,
  thresholds: DEFAULT_THRESHOLDS,
  settlements: [],
};

const catalogRaw = JSON.parse(
  readFileSync(new URL('../../../webapp/public/.well-known/rgb/tokens.json', import.meta.url), 'utf8'),
);

test('reputation event: the tags a client filters on', () => {
  const e = signer.sign(reputationTemplate(score, 'signetcustom', NOW));
  assert.equal(e.kind, REPUTATION_KIND);
  const t = Object.fromEntries(e.tags.map((x) => [x[0], x[1]]));
  assert.deepEqual(t, {
    d: MAKER,
    p: MAKER,
    t: 'mosaik-reputation-v1',
    network: 'signetcustom',
    window: '2592000',
    settled: '3',
    volume_sat: '120000',
    counterparties: '2',
    self_trades: '1',
    negotiations: '6',
    aborted: '1',
    verified: '1',
    expiration: String(NOW + REPUTATION_TTL_SECS),
    presence: '0.9731',
    abort_rate: '0.2000',
  });
});

test('reputation event round-trips through the verifier', () => {
  const e = signer.sign(reputationTemplate(score, 'signetcustom', NOW));
  const r = verifyReputationEvent(e, signer.pubkey, NOW + 60);
  assert.ok(r.ok, !r.ok ? r.error : '');
  assert.equal(r.value.maker, MAKER);
  assert.equal(r.value.volumeSat, 120_000);
  assert.equal(r.value.presence, 0.9731);
  assert.equal(r.value.abortRate, 0.2);
  assert.equal(r.value.verified, true);
});

test('reputation event: wrong signer, tampering and expiry are refused', () => {
  const e = signer.sign(reputationTemplate(score, 'signetcustom', NOW));
  assert.equal(verifyReputationEvent(e, other.pubkey, NOW).ok, false);
  const forged = { ...e, tags: e.tags.map((t) => (t[0] === 'verified' ? ['verified', '1'] : t[0] === 'settled' ? ['settled', '99'] : t)) };
  assert.equal(verifyReputationEvent(forged, signer.pubkey, NOW).ok, false);
  // re-signed by the indexer but with tags that disagree with the content
  const inconsistent = signer.sign({ ...reputationTemplate(score, 'signetcustom', NOW), tags: forged.tags });
  assert.equal(verifyReputationEvent(inconsistent, signer.pubkey, NOW).ok, false);
  assert.equal(verifyReputationEvent(e, signer.pubkey, NOW + REPUTATION_TTL_SECS + 1).ok, false);
});

test('a maker with no observations has no presence or abort-rate tag', () => {
  const e = signer.sign(
    reputationTemplate({ ...score, presence: { ...score.presence, ratio: null }, negotiations: { ...score.negotiations, abortRate: null } }, 'signetcustom', NOW),
  );
  assert.equal(e.tags.some((t) => t[0] === 'presence' || t[0] === 'abort_rate'), false);
  const r = verifyReputationEvent(e, signer.pubkey, NOW);
  assert.ok(r.ok);
  assert.equal(r.value.presence, null);
});

test('the repository catalog is a valid mosaik-catalog/v1', () => {
  const c = parseCatalog(catalogRaw);
  assert.equal(c.network, 'signetcustom');
  assert.equal(c.tokens.length, 2);
});

test('catalog checks: duplicate ids, duplicate tickers, bad issuer makers', () => {
  const t0 = catalogRaw.tokens[0];
  assert.throws(() => parseCatalog({ ...catalogRaw, tokens: [t0, t0] }), /twice/);
  assert.throws(() => parseCatalog({ ...catalogRaw, tokens: [t0, { ...catalogRaw.tokens[1], ticker: t0.ticker.toLowerCase() }] }), /ticker/);
  assert.throws(() => parseCatalog({ ...catalogRaw, tokens: [{ ...t0, issuer_makers: ['npub1...'] }] }), /issuer_makers/);
  assert.throws(() => parseCatalog({ ...catalogRaw, format: 'other' }), /format/);
});

test('curated list event round-trips and carries the catalog verbatim', () => {
  const c = parseCatalog({ ...catalogRaw, tokens: [{ ...catalogRaw.tokens[0], issuer_makers: [MAKER] }, catalogRaw.tokens[1]] });
  const e = signer.sign(catalogTemplate(c, NOW));
  assert.equal(e.kind, CATALOG_KIND);
  assert.deepEqual(e.tags.slice(0, 4), [
    ['d', 'mosaik-catalog:signetcustom'],
    ['t', 'mosaik-catalog-v1'],
    ['network', 'signetcustom'],
    ['version', '0.1.0'],
  ]);
  assert.deepEqual(e.tags.filter((t) => t[0] !== 'd' && t[0] !== 't' && t[0] !== 'network' && t[0] !== 'version'), [
    ['asset', c.tokens[0]!.contract_id, 'TESTUSD'],
    ['issuer_maker', c.tokens[0]!.contract_id, MAKER],
    ['asset', c.tokens[1]!.contract_id, 'TESTXAU'],
  ]);
  const r = verifyCatalogEvent(e, signer.pubkey, 'signetcustom');
  assert.ok(r.ok, !r.ok ? r.error : '');
  assert.deepEqual(r.value, c);
});

test('curated list: another key, another network, or tags that lie are refused', () => {
  const c = parseCatalog(catalogRaw);
  const e = signer.sign(catalogTemplate(c, NOW));
  assert.equal(verifyCatalogEvent(e, other.pubkey, 'signetcustom').ok, false);
  assert.equal(verifyCatalogEvent(e, signer.pubkey, 'bitcoin').ok, false);
  const tpl = catalogTemplate(c, NOW);
  const lying = signer.sign({ ...tpl, tags: tpl.tags.filter((t) => !(t[0] === 'asset' && t[2] === 'TESTXAU')) });
  assert.equal(verifyCatalogEvent(lying, signer.pubkey, 'signetcustom').ok, false);
});
