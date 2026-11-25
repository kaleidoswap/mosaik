/** Thin wrappers over nostr-tools, so the rest of the service deals in plain objects. */

import { finalizeEvent, getPublicKey, verifyEvent, type Event, type EventTemplate } from 'nostr-tools/pure';
import { hexToBytes, bytesToHex } from 'nostr-tools/utils';
import * as nip19 from 'nostr-tools/nip19';

export type NostrEvent = Event;
export type Template = EventTemplate;

export interface Signer {
  pubkey: string;
  npub: string;
  sign(t: Template): NostrEvent;
}

export function signerFromHex(secretHex: string): Signer {
  if (!/^[0-9a-f]{64}$/.test(secretHex)) throw new Error('the signing key must be 32 bytes of lowercase hex');
  const sk = hexToBytes(secretHex);
  const pubkey = getPublicKey(sk);
  return { pubkey, npub: nip19.npubEncode(pubkey), sign: (t) => finalizeEvent(t, sk) };
}

/**
 * Checks the id and the signature: a relay hands over whatever anyone signed, or did not.
 * nostr-tools caches a verdict on the event object, and a spread copy carries it over; checking a
 * fresh plain copy means only the fields themselves decide.
 */
export const isValidEvent = (e: unknown): e is NostrEvent => {
  try {
    const { id, pubkey, created_at, kind, tags, content, sig } = e as NostrEvent;
    return verifyEvent({ id, pubkey, created_at, kind, tags, content, sig } as NostrEvent);
  } catch {
    return false;
  }
};

export { bytesToHex, hexToBytes };
