/**
 * banco-proxy's wrapper around `@arkade-os/banco`.
 *
 * Drives the real upstream SDK directly: `MnemonicIdentity` → `Wallet.create`
 * → `Maker` / `Taker`. No stubs, no toggle. Wallets are loaded per-request
 * from `BANCO_WALLET_SEED_<ID>` env vars (a dev convenience — production
 * deployments should replace `getWallet` with a KMS / secret-store call).
 */

import {
  ArkAddress,
  MnemonicIdentity,
  RestIntrospectorProvider,
  Wallet,
  asset,
  networks,
  type IWallet,
  type Network,
} from '@arkade-os/sdk';
import { Maker, Taker, type OfferStatus } from '@arkade-os/banco';
import { hex } from '@scure/base';

const ARK_SERVER_URL = process.env.ARK_SERVER_URL ?? 'http://arkd:7070';
const INTROSPECTOR_URL = process.env.INTROSPECTOR_URL ?? 'http://introspector:7073';

// Default network. mutinynet defaults the dev stack; flip to `bitcoin` for
// mainnet via BANCO_NETWORK=bitcoin once the SDK supports it.
const NETWORK_NAME = (process.env.BANCO_NETWORK ?? 'mutinynet').toLowerCase();
const NETWORK: Network =
  (networks as Record<string, Network>)[NETWORK_NAME] ?? networks.mutinynet;
const IS_MAINNET = NETWORK_NAME === 'bitcoin';

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

const walletCache = new Map<string, Wallet>();

async function getWallet(walletId: string): Promise<Wallet> {
  const cached = walletCache.get(walletId);
  if (cached) return cached;

  const envKey = `BANCO_WALLET_SEED_${walletId.toUpperCase().replace(/[^A-Z0-9]/g, '_')}`;
  const mnemonic = process.env[envKey];
  if (!mnemonic) {
    throw new Error(
      `no wallet seed configured for walletId='${walletId}' (looked for ${envKey})`,
    );
  }

  const identity = MnemonicIdentity.fromMnemonic(mnemonic, { isMainnet: IS_MAINNET });
  const wallet = await Wallet.create({ identity, arkServerUrl: ARK_SERVER_URL });
  walletCache.set(walletId, wallet);
  return wallet;
}

// ─── Identity ───────────────────────────────────────────────────────────────

export async function identity(walletId: string): Promise<Identity> {
  const wallet = await getWallet(walletId);
  const walletPubkey = hex.encode(await wallet.identity.xOnlyPublicKey());

  const arkInfo = await wallet.arkProvider.getInfo();

  // The introspector pubkey lives on the introspector itself, not on arkd.
  const introspector = new RestIntrospectorProvider(INTROSPECTOR_URL);
  const introspectorInfo = await introspector.getInfo();

  return {
    walletPubkey,
    arkServerPubkey: stripCompressed(arkInfo.signerPubkey),
    introspectorPubkey: stripCompressed(introspectorInfo.signerPubkey),
  };
}

// ─── Maker ──────────────────────────────────────────────────────────────────

export async function createOffer(p: CreateOfferParams): Promise<BuiltOffer> {
  const wallet = await getWallet(p.walletId);
  const maker = new Maker(wallet as IWallet, ARK_SERVER_URL, INTROSPECTOR_URL);

  // banco SDK uses presence of ratioNum/ratioDen as the partial-fill signal.
  // When partialFill=true the natural ratio is sell:want; when false, omit.
  const ratioFields = p.partialFill
    ? { ratioNum: BigInt(p.sellAmount), ratioDen: BigInt(p.wantAmount) }
    : {};

  const { offer, packet, swapPkScript } = await maker.createOffer({
    wantAmount: BigInt(p.wantAmount),
    wantAsset: p.wantAsset ? asset.AssetId.fromString(p.wantAsset) : undefined,
    offerAsset: p.sellAsset ? asset.AssetId.fromString(p.sellAsset) : undefined,
    cancelDelay: p.cancelDelay,
    ...ratioFields,
  });

  // Fund the swap address with `sellAmount` sats and embed the offer packet
  // in the funding tx's Arkade extension output. wallet.send returns the
  // ark transaction id.
  const arkInfo = await wallet.arkProvider.getInfo();
  const serverPubKey = stripCompressedBytes(hex.decode(arkInfo.signerPubkey));
  const swapAddress = swapPkScriptToArkAddress(swapPkScript, serverPubKey);
  const fundingTxid = await wallet.send({
    address: swapAddress,
    amount: p.sellAmount,
    extensions: [{ type: packet.type, payload: packet.payload }],
  });

  return {
    offerHex: offer,
    swapPkScript: hex.encode(swapPkScript),
    fundingTxid,
    // banco places the swap output at index 0 by convention; if a future
    // SDK release shifts this, read the real vout from the wallet response.
    fundingVout: 0,
  };
}

export async function cancelOffer(walletId: string, offerHex: string): Promise<string> {
  const wallet = await getWallet(walletId);
  const maker = new Maker(wallet as IWallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  return await maker.cancelOffer(offerHex);
}

export async function listMakerOffers(
  walletId: string,
  swapPkScript: string,
): Promise<MakerOffer[]> {
  const wallet = await getWallet(walletId);
  const maker = new Maker(wallet as IWallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  const offers = await maker.getOffers(hex.decode(swapPkScript));
  return offers.map((o: OfferStatus) => ({
    txid: o.txid,
    vout: o.vout,
    value: o.value,
    spendable: o.spendable,
  }));
}

// ─── Taker ──────────────────────────────────────────────────────────────────

export async function fulfill(p: FulfillParams): Promise<string> {
  const wallet = await getWallet(p.walletId);
  const taker = new Taker(wallet as IWallet, ARK_SERVER_URL, INTROSPECTOR_URL);
  const opts =
    p.fillAmount !== undefined ? { fillAmount: BigInt(p.fillAmount) } : undefined;
  const { txid } = p.offerHex
    ? await taker.fulfill(p.offerHex, opts)
    : await taker.fulfillByTxid(p.txid!, opts);
  return txid;
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/** Strip the leading 0x02/0x03 prefix from a 33-byte compressed pubkey hex. */
function stripCompressed(input: string | Uint8Array): string {
  const bytes = typeof input === 'string' ? hex.decode(input) : input;
  return hex.encode(stripCompressedBytes(bytes));
}

/** Strip the leading 0x02/0x03 prefix from a 33-byte compressed pubkey. */
function stripCompressedBytes(bytes: Uint8Array): Uint8Array {
  return bytes.length === 33 ? bytes.slice(1) : bytes;
}

/**
 * Derive the Arkade bech32m swap address from banco's `swapPkScript`.
 *
 * banco returns a Taproot pkScript (`OP_1 32 <vtxoTaprootKey>` = 34 bytes).
 * `ArkAddress` is `(serverPubKey, vtxoTaprootKey, hrp, version)`; we take
 * the trailing 32 bytes to recover the taproot output key.
 */
function swapPkScriptToArkAddress(swapPkScript: Uint8Array, serverPubKey: Uint8Array): string {
  if (swapPkScript.length < 32) {
    throw new Error(`swapPkScript too short: ${swapPkScript.length} bytes`);
  }
  const vtxoTaprootKey = swapPkScript.slice(-32);
  const addr = new ArkAddress(serverPubKey, vtxoTaprootKey, NETWORK.hrp);
  return addr.encode();
}
