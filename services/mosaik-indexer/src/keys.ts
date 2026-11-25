/**
 * The indexer's signing key. Development only: a Mutinynet key in a file outside the repository.
 * Which key signs in production, and who holds it, is an open decision (see README).
 */

import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { generateSecretKey } from 'nostr-tools/pure';
import { bytesToHex, signerFromHex, type Signer } from './nostr.ts';
import type { Config } from './config.ts';

export function loadSigner(cfg: Pick<Config, 'secretHex' | 'keyFile'>): Signer {
  if (cfg.secretHex) return signerFromHex(cfg.secretHex.trim());
  let raw: string;
  try {
    raw = readFileSync(cfg.keyFile, 'utf8');
  } catch {
    throw new Error(`no signing key at ${cfg.keyFile}: run \`pnpm keygen\` or set MOSAIK_INDEXER_SECRET_HEX`);
  }
  const mode = statSync(cfg.keyFile).mode & 0o077;
  if (mode !== 0) throw new Error(`${cfg.keyFile} is readable by others; chmod 600 it`);
  return signerFromHex(raw.trim());
}

/** Write a fresh key; never overwrites one. */
export function generateKeyFile(path: string): Signer {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  const hex = bytesToHex(generateSecretKey());
  writeFileSync(path, hex + '\n', { mode: 0o600, flag: 'wx' });
  return signerFromHex(hex);
}
