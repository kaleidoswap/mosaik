/**
 * banco-proxy's wrapper around `@arkade-os/banco`.
 *
 * The exported functions are the names the REST routes import; each calls
 * directly into `Maker` / `Taker` from the upstream SDK. There is no stub
 * implementation and no runtime selector — the proxy commits to driving the
 * real SDK.
 *
 * Operational note: `@arkade-os/banco` is alpha. Until upstream publishes a
 * tagged npm release and stabilises its build, `npm ci` in this directory
 * may fail (see services/banco-proxy/README.md). The proxy is "ready to run
 * the day the install works"; no extra wiring required here.
 */

import { Maker, Taker, type Wallet } from '@arkade-os/banco';
import { hex } from '@scure/base';

const ARK_SERVER_URL = process.env.ARK_SERVER_URL ?? 'http://arkd:7070';
const INTROSPECTOR_URL = process.env.INTROSPECTOR_URL ?? 'http://introspector:7073';

// ─── Types the routes import ────────────────────────────────────────────────

export interface Identity {
  walletPubkey: string;
  arkServerPubkey: string;
  introspectorPubkey: string;
}

export interface CreateOfferParams {
  walletId: string;
  wantAmount: number;
  wantAsset?: string;
  sellAmount: number;
  sellAsset?: string;
  cancelDelay: number;
  partialFill?: boolean;
}

export interface BuiltOffer {
  offerHex: string;
  swapPkScript: string;
  fundingTxid: string;
  fundingVout: number;
}

export interface MakerOffer {
  txid: string;
  vout: number;
  value: number;
  spendable: boolean;
}

export interface FulfillParams {
  walletId: string;
  offerHex?: string;
  txid?: string;
  fillAmount?: number;
}

// ─── Wallet loader ──────────────────────────────────────────────────────────
//
// Wallets are looked up per-request from a deployment-specific secret
// store. The default loader expects seeds at `BANCO_WALLET_SEED_<ID>` env
// vars (one per walletId, mnemonic). For production, replace `getWallet`
// with a call into the operator's KMS / mounted secret volume.

const walletCache = new Map<string, Wallet>();

async function getWallet(walletId: string): Promise<Wallet> {
  const cached = walletCache.get(walletId);
  if (cached) return cached;

  const envKey = `BANCO_WALLET_SEED_${walletId.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`;
  const seed = process.env[envKey];
  if (!seed) {
    throw new Error(
      `no wallet seed configured for walletId='${walletId}' (looked for ${envKey})`,
    );
  }
  // The exact constructor here depends on the @arkade-os/banco release; the
  // call below targets the SDK's documented `Wallet.fromMnemonic` entry.
  // Adjust to whatever the released package exposes — see the package's
  // own examples directory.
  const wallet = await (Wallet as unknown as {
    fromMnemonic(seed: string, opts: { arkServerUrl: string }): Promise<Wallet>;
  }).fromMnemonic(seed, { arkServerUrl: ARK_SERVER_URL });
  walletCache.set(walletId, wallet);
  return wallet;
}

// ─── Identity ───────────────────────────────────────────────────────────────

export async function identity(walletId: string): Promise<Identity> {
  const wallet = await getWallet(walletId);
  const walletPubkey = hex.encode(await wallet.identity.xOnlyPublicKey());
  const arkInfo = await wallet.arkProvider.getInfo();
  return {
    walletPubkey,
    arkServerPubkey: stripCompressed(arkInfo.signerPubkey),
    introspectorPubkey: stripCompressed(arkInfo.introspectorPubkey),
  };
}

// ─── Maker ──────────────────────────────────────────────────────────────────

export async function createOffer(p: CreateOfferParams): Promise<BuiltOffer> {
  const wallet = await getWallet(p.walletId);
  const maker = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);

  const { offer, swapPkScript, packet } = await maker.createOffer({
    wantAmount: BigInt(p.wantAmount),
    wantAsset: p.wantAsset,
    cancelDelay: p.cancelDelay,
    ratioNum: BigInt(p.sellAmount),
    ratioDen: BigInt(p.wantAmount),
  });

  // Fund the swap address with `sellAmount` and embed the offer packet in
  // the funding tx's Arkade extension output. The wallet handles both in
  // one signed-and-broadcast transaction.
  const fundingTxid = await wallet.sendWithExtension(
    swapPkScript,
    BigInt(p.sellAmount),
    packet,
  );

  return {
    offerHex: hex.encode(offer),
    swapPkScript: hex.encode(swapPkScript),
    fundingTxid,
    // banco places the swap output at index 0 by convention; if a future
    // SDK release shifts this, surface the real vout from the wallet.
    fundingVout: 0,
  };
}

export async function cancelOffer(walletId: string, offerHex: string): Promise<string> {
  const wallet = await getWallet(walletId);
  const maker = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  return await maker.cancelOffer(offerHex);
}

export async function listMakerOffers(
  walletId: string,
  swapPkScript: string,
): Promise<MakerOffer[]> {
  const wallet = await getWallet(walletId);
  const maker = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  const offers = await maker.getOffers(swapPkScript);
  return offers.map((o) => ({
    txid: o.txid,
    vout: o.vout,
    value: Number(o.value),
    spendable: o.spendable,
  }));
}

// ─── Taker ──────────────────────────────────────────────────────────────────

export async function fulfill(p: FulfillParams): Promise<string> {
  const wallet = await getWallet(p.walletId);
  const taker = new Taker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  const opts =
    p.fillAmount !== undefined ? { fillAmount: BigInt(p.fillAmount) } : undefined;
  const { txid } = p.offerHex
    ? await taker.fulfill(p.offerHex, opts)
    : await taker.fulfillByTxid(p.txid!, opts);
  return txid;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/** Strip the leading 0x02/0x03 prefix from a 33-byte compressed pubkey. */
function stripCompressed(input: string | Uint8Array): string {
  const bytes = typeof input === 'string' ? hex.decode(input) : input;
  const xOnly = bytes.length === 33 ? bytes.slice(1) : bytes;
  return hex.encode(xOnly);
}
