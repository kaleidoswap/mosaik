/**
 * mosaik-indexer
 *
 *   run     subscribe, check settlements, serve HTTP, sign (and publish when MOSAIK_INDEXER_PUBLISH=1)
 *   once    read what the relay holds now, check settlements, print the scores and the signed events
 *   keygen  write a fresh Mutinynet dev key to MOSAIK_INDEXER_KEY_FILE (never overwrites)
 *   pubkey  print the signing pubkey
 */

import { loadConfig } from './config.ts';
import { generateKeyFile, loadSigner } from './keys.ts';
import { Store } from './store.ts';
import { Esplora } from './esplora.ts';
import { Indexer } from './indexer.ts';
import { startHttp } from './http.ts';
import { log } from './log.ts';

async function main() {
  const cmd = process.argv[2] ?? 'run';
  const cfg = loadConfig();

  if (cmd === 'keygen') {
    const s = generateKeyFile(cfg.keyFile);
    console.log(JSON.stringify({ keyFile: cfg.keyFile, pubkey: s.pubkey, npub: s.npub }, null, 2));
    return;
  }
  const signer = loadSigner(cfg);
  if (cmd === 'pubkey') {
    console.log(JSON.stringify({ pubkey: signer.pubkey, npub: signer.npub }, null, 2));
    return;
  }

  const store = new Store(cfg.dbPath);
  const indexer = new Indexer(store, new Esplora(cfg.esploraUrl), signer, cfg);

  if (cmd === 'once') {
    await new Promise<void>((resolve, reject) => {
      const t = setTimeout(() => reject(new Error('relay did not answer in 30 s')), 30_000);
      indexer.subscribe(() => {
        clearTimeout(t);
        resolve();
      });
      indexer.relay.start();
    });
    store.markCovered(Math.floor(Date.now() / 1000));
    // checks that are not yet due are run now as well: this is a one-shot look
    await indexer.runChecks(Number.MAX_SAFE_INTEGER, 1000);
    const events = await indexer.signAndPublish();
    indexer.relay.stop();
    console.log(JSON.stringify({ indexer: signer.pubkey, scores: indexer.scores(), events }, null, 2));
    store.close();
    return;
  }

  if (cmd !== 'run') throw new Error(`unknown command ${cmd}`);
  log.info('starting', {
    network: cfg.network,
    relay: cfg.relayUrl,
    esplora: cfg.esploraUrl,
    db: cfg.dbPath,
    pubkey: signer.pubkey,
    publish: cfg.publish,
    http: `${cfg.httpHost}:${cfg.httpPort}`,
  });
  indexer.start();
  const server = startHttp(indexer, store, signer, cfg);
  // first signing once the book has had a moment to arrive
  setTimeout(() => void indexer.signAndPublish().catch((e) => log.error('sign failed', { error: String(e) })), 15_000);
  const stop = () => {
    log.info('stopping');
    indexer.stop();
    server.close();
    store.close();
    process.exit(0);
  };
  process.on('SIGINT', stop);
  process.on('SIGTERM', stop);
}

main().catch((e) => {
  log.error(String(e instanceof Error ? e.message : e));
  process.exit(1);
});
