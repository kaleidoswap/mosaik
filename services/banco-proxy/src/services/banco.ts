/**
 * banco service — the only file that imports @arkade-os/banco.
 *
 * Today every method is stubbed: it returns a deterministic, well-shaped
 * placeholder response so the HTTP contract is exercisable end-to-end
 * (Rust → banco-proxy → JSON) without needing a live arkd + introspector +
 * banco SDK install. When upstream banco stabilises and arkd is reachable,
 * swap the stubs for real `new Maker(...)` / `new Taker(...)` calls.
 *
 * To turn this into the real implementation:
 *   1. `pnpm add @arkade-os/banco @arkade-os/sdk`
 *   2. Replace each `STUB:` block with the SDK call indicated.
 *   3. Wire wallet construction (one Wallet per walletId) in `getWallet()`.
 *
 * The function signatures match what the routes expect; do not change them
 * without updating crates/mosaik-banco/src/client.rs accordingly.
 */

import { createHash, randomBytes } from 'node:crypto';

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

/** STUB: replace with a Wallet constructed from the wallet seed for `walletId`. */
function deterministicHex(seed: string, bytes: number): string {
  return createHash('sha256').update(seed).digest('hex').slice(0, bytes * 2);
}

export async function identity(walletId: string): Promise<Identity> {
  // STUB: real impl:
  //   const wallet = await getWallet(walletId);
  //   const arkClient = ArkClient.fromUrl(process.env.ARK_SERVER_URL!);
  //   const info = await arkClient.getInfo();
  //   return {
  //     walletPubkey: hex(await wallet.identity.xOnlyPublicKey()),
  //     arkServerPubkey: hex(info.signerPubkey).slice(2),
  //     introspectorPubkey: hex(info.introspectorPubkey).slice(2),
  //   };
  return {
    walletPubkey: deterministicHex(`wallet:${walletId}`, 32),
    arkServerPubkey: deterministicHex('arkd:signer', 32),
    introspectorPubkey: deterministicHex('introspector', 32),
  };
}

export async function createOffer(p: CreateOfferParams): Promise<BuiltOffer> {
  // STUB: real impl:
  //   const wallet = await getWallet(p.walletId);
  //   const maker  = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  //   const { offer, swapPkScript, packet } = await maker.createOffer({
  //     wantAmount: BigInt(p.wantAmount),
  //     wantAsset:  p.wantAsset ? AssetId.fromHex(p.wantAsset) : undefined,
  //     cancelDelay: p.cancelDelay,
  //     ratioNum:    BigInt(p.sellAmount),
  //     ratioDen:    BigInt(p.wantAmount),
  //   });
  //   // fund swapPkScript with sellAmount + embed `packet` as extension type 0x03
  //   const fundingTxid = await wallet.sendWithExtension(swapPkScript, BigInt(p.sellAmount), packet);
  //   return { offerHex: hex(offer), swapPkScript: hex(swapPkScript), fundingTxid, fundingVout: 0 };
  const seed = `${p.walletId}|${p.sellAmount}|${p.wantAmount}|${p.cancelDelay}|${Date.now()}`;
  return {
    offerHex: deterministicHex(`offer:${seed}`, 64),
    swapPkScript: deterministicHex(`spk:${seed}`, 34),
    fundingTxid: deterministicHex(`fund:${seed}`, 32),
    fundingVout: 0,
  };
}

export async function cancelOffer(walletId: string, offerHex: string): Promise<string> {
  // STUB: real impl:
  //   const wallet = await getWallet(walletId);
  //   const maker  = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  //   return await maker.cancelOffer(offerHex);
  return deterministicHex(`cancel:${walletId}:${offerHex}`, 32);
}

export async function listMakerOffers(
  walletId: string,
  swapPkScript: string,
): Promise<MakerOffer[]> {
  // STUB: real impl:
  //   const wallet = await getWallet(walletId);
  //   const maker  = new Maker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  //   return await maker.getOffers(swapPkScript);
  return [
    {
      txid: deterministicHex(`stub:${walletId}:${swapPkScript}`, 32),
      vout: 0,
      value: 0,
      spendable: false,
    },
  ];
}

export async function fulfill(p: FulfillParams): Promise<string> {
  // STUB: real impl:
  //   const wallet = await getWallet(p.walletId);
  //   const taker  = new Taker(wallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  //   const opts   = p.fillAmount !== undefined ? { fillAmount: BigInt(p.fillAmount) } : undefined;
  //   const { txid } = p.offerHex
  //     ? await taker.fulfill(p.offerHex, opts)
  //     : await taker.fulfillByTxid(p.txid!, opts);
  //   return txid;
  const key = p.offerHex ?? p.txid ?? randomBytes(16).toString('hex');
  return deterministicHex(`fulfill:${p.walletId}:${key}:${p.fillAmount ?? 'full'}`, 32);
}
