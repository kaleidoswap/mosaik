import express from 'express';
import { authenticateJWT, type AuthedRequest } from '../auth.js';
import * as banco from '../services/banco.js';
import { logger } from '../logger.js';

export function createBancoRoutes(): express.Router {
  const router = express.Router();

  // Every route below requires a wallet token.
  router.use(authenticateJWT);

  // ─── Identity ──────────────────────────────────────────────────────────────
  router.get('/identity', async (req, res) => {
    try {
      res.json(await banco.identity((req as AuthedRequest).wallet.id));
    } catch (e) {
      handle(res, e, 'identity');
    }
  });

  // ─── Maker ─────────────────────────────────────────────────────────────────
  router.post('/maker/offer', async (req, res) => {
    try {
      const { wantAmount, wantAsset, sellAmount, sellAsset, cancelDelay, partialFill } =
        req.body ?? {};
      if (typeof wantAmount !== 'number' || typeof sellAmount !== 'number') {
        res.status(400).json({ error: 'wantAmount and sellAmount must be numbers' });
        return;
      }
      if (typeof cancelDelay !== 'number') {
        res.status(400).json({ error: 'cancelDelay must be a number (unix seconds)' });
        return;
      }
      const out = await banco.createOffer({
        walletId: (req as AuthedRequest).wallet.id,
        wantAmount,
        wantAsset,
        sellAmount,
        sellAsset,
        cancelDelay,
        partialFill: !!partialFill,
      });
      res.json(out);
    } catch (e) {
      handle(res, e, 'maker/offer');
    }
  });

  router.post('/maker/cancel', async (req, res) => {
    try {
      const { offerHex } = req.body ?? {};
      if (!offerHex) {
        res.status(400).json({ error: 'offerHex is required' });
        return;
      }
      const txid = await banco.cancelOffer((req as AuthedRequest).wallet.id, offerHex);
      res.json({ txid });
    } catch (e) {
      handle(res, e, 'maker/cancel');
    }
  });

  router.get('/maker/offers', async (req, res) => {
    try {
      const swapPkScript = req.query.swapPkScript;
      if (typeof swapPkScript !== 'string') {
        res.status(400).json({ error: 'swapPkScript query param is required' });
        return;
      }
      const offers = await banco.listMakerOffers(
        (req as AuthedRequest).wallet.id,
        swapPkScript,
      );
      res.json(offers);
    } catch (e) {
      handle(res, e, 'maker/offers');
    }
  });

  // ─── Taker ─────────────────────────────────────────────────────────────────
  router.post('/taker/fulfill', async (req, res) => {
    try {
      const { offerHex, fillAmount } = req.body ?? {};
      if (!offerHex) {
        res.status(400).json({ error: 'offerHex is required' });
        return;
      }
      const txid = await banco.fulfill({
        walletId: (req as AuthedRequest).wallet.id,
        offerHex,
        fillAmount,
      });
      res.json({ txid });
    } catch (e) {
      handle(res, e, 'taker/fulfill');
    }
  });

  router.post('/taker/fulfill-by-txid', async (req, res) => {
    try {
      const { txid, fillAmount } = req.body ?? {};
      if (!txid) {
        res.status(400).json({ error: 'txid is required' });
        return;
      }
      const settle = await banco.fulfill({
        walletId: (req as AuthedRequest).wallet.id,
        txid,
        fillAmount,
      });
      res.json({ txid: settle });
    } catch (e) {
      handle(res, e, 'taker/fulfill-by-txid');
    }
  });

  return router;
}

function handle(res: express.Response, e: unknown, where: string): void {
  const msg = e instanceof Error ? e.message : String(e);
  logger.warn({ err: e, where }, 'banco route failed');
  res.status(500).json({ error: msg });
}
