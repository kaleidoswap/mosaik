/**
 * JWT auth — same shape as `ark-proxy` so callers (mosaik-banco, the eventual
 * webapp) can reuse the auth flow.
 *
 *   POST /api/v1/auth/token  { walletId }  →  { token, walletId }
 *   then every /banco/* call carries `Authorization: Bearer <token>`.
 *
 * Secret comes from BANCO_PROXY_JWT_SECRET; defaults to a dev-only string so
 * the proxy boots in tests without ceremony. NEVER ship that default to prod.
 */

import jwt from 'jsonwebtoken';
import type { Request, Response, NextFunction } from 'express';

const SECRET =
  process.env.BANCO_PROXY_JWT_SECRET ??
  'dev-only-secret-do-not-use-in-production-please';

const EXPIRES_IN = process.env.JWT_EXPIRES_IN ?? '24h';

/** Wallets the proxy knows about. In dev they map to fixed banco-SDK wallets;
 *  in production this list comes from a mounted seed store. */
const KNOWN_WALLETS = new Set(['maker', 'taker', 'mm-a', 'mm-b']);

export interface AuthedRequest extends Request {
  wallet: { id: string };
}

export function generateToken(walletId: string): string {
  if (!KNOWN_WALLETS.has(walletId)) {
    throw new Error(`unknown wallet '${walletId}'`);
  }
  return jwt.sign({ walletId }, SECRET, { expiresIn: EXPIRES_IN } as jwt.SignOptions);
}

export function authenticateJWT(req: Request, res: Response, next: NextFunction): void {
  const header = req.headers.authorization;
  if (!header?.startsWith('Bearer ')) {
    res.status(401).json({ error: 'missing bearer token' });
    return;
  }
  try {
    const payload = jwt.verify(header.slice(7), SECRET) as { walletId: string };
    (req as AuthedRequest).wallet = { id: payload.walletId };
    next();
  } catch (e) {
    res.status(401).json({ error: `invalid token: ${(e as Error).message}` });
  }
}
