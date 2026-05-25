import express from 'express';
import { generateToken } from '../auth.js';
import { logger } from '../logger.js';

export function createAuthRoutes(): express.Router {
  const router = express.Router();

  router.post('/token', (req, res) => {
    try {
      const { walletId } = req.body as { walletId?: string };
      if (!walletId) {
        res.status(400).json({ error: 'walletId is required' });
        return;
      }
      const token = generateToken(walletId);
      res.json({ token, walletId, expiresIn: process.env.JWT_EXPIRES_IN ?? '24h' });
    } catch (e) {
      logger.warn({ err: e }, 'token issue failed');
      res.status(400).json({ error: (e as Error).message });
    }
  });

  return router;
}
