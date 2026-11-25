import { finalizeEvent, generateSecretKey, getPublicKey, nip44, Relay, verifyEvent, type Event, type Filter } from 'nostr-tools';
import type { BrowserProvider } from './webrgb';

export const PHONE_RELAY = 'wss://relay.kaleidoswap.com';
const NETWORK = 'mutinynet';
const REQUIRED = ['get_info', 'kaleido_get_btc_balance', 'kaleido_list_assets'];
const PREPARE = 'kaleido_create_utxos';
const INFO = 13194;
const REQUEST = 23194;
const RESPONSE = 23195;
const now = () => Math.floor(Date.now() / 1000);
const nonce = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), x => x.toString(16).padStart(2, '0')).join('');
const record = (x: unknown): x is Record<string, unknown> => x !== null && typeof x === 'object' && !Array.isArray(x);
const field = (e: Event, name: string) => {
  const tags = e.tags.filter(t => t[0] === name);
  return tags.length === 1 && tags[0]?.length === 2 ? tags[0][1] : undefined;
};
const words = (s: string | undefined) => (s ?? '').split(/\s+/);

export interface PhoneRelay {
  readonly connected: boolean;
  connect(opts?: {timeout: number}): Promise<void>;
  subscribe(filters: Filter[], options: {onevent: (e: Event) => void}): {close(): void};
  publish(e: Event): Promise<string>;
  close(): void;
}

type Balance = Awaited<ReturnType<NonNullable<BrowserProvider['getBtcBalance']>>>;
function validateBalance(value: unknown): Balance {
  const sats = (v: unknown) => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0;
  if (!record(value) || !record(value.vanilla) || !record(value.colored)
    || ![value.vanilla.spendable, value.vanilla.future, value.vanilla.settled, value.colored.settled].every(sats)
    || (value.freeColorableUtxos !== null && !sats(value.freeColorableUtxos))) throw new Error('Invalid Bitcoin balance from the phone.');
  return value as Balance;
}

export class PhoneNwcSession {
  private readonly secret = generateSecretKey();
  private readonly pubkey = getPublicKey(this.secret);
  private readonly state = nonce();
  private readonly challenge = nonce();
  private readonly started = now();
  private readonly expires = this.started + 3600;
  private readonly stop = new AbortController();
  private subscription?: {close(): void};
  private expiryTimer?: ReturnType<typeof setTimeout>;
  private wallet?: string;
  private key?: Uint8Array;
  private offered: string[] = [];
  private readonly pending = new Map<string, {method: string; started: number; expires: number; reply: (payload: Record<string, unknown>) => void}>();

  constructor(
    private readonly relay: PhoneRelay = new Relay(PHONE_RELAY, {enableReconnect: true, enablePing: true}),
    private readonly onClosed?: (reason: string) => void,
  ) {}

  close(reason = 'Phone wallet disconnected.') {
    if (this.stop.signal.aborted) return;
    this.stop.abort(new Error(reason));
    clearTimeout(this.expiryTimer);
    this.subscription?.close();
    this.relay.close();
    this.secret.fill(0);
    this.key?.fill(0);
    this.onClosed?.(reason);
  }

  private async pair(showQr: (uri: string) => void) {
    await this.relay.connect({timeout: 10_000});
    if (this.stop.signal.aborted) { this.relay.close(); throw this.stop.signal.reason; }
    await new Promise<void>((resolve, reject) => {
      const abort = () => reject(this.stop.signal.reason);
      this.stop.signal.addEventListener('abort', abort, {once: true});
      this.expiryTimer = setTimeout(() => this.close('Pairing expired. Scan a new QR.'), 180_000);
      this.subscription = this.relay.subscribe([{kinds: [INFO, RESPONSE], '#p': [this.pubkey], since: this.started - 30}], {
        onevent: e => {
          if (this.stop.signal.aborted || now() >= this.expires || (!this.wallet && now() > this.started + 180)
            || !Number.isSafeInteger(e.created_at) || e.created_at < this.started - 30 || e.created_at > now() + 30
            || field(e, 'p') !== this.pubkey || !verifyEvent(e)) return;
          if (this.wallet) { this.receive(e); return; }
          if (e.kind !== INFO || field(e, 'state') !== this.state || !words(field(e, 'extensions')).includes('08')
            || !words(field(e, 'encryption')).includes('nip44_v2')) return;
          const proof = field(e, 'proof');
          const methods = words(e.content);
          if (!proof || proof.length > 2048 || !REQUIRED.every(method => methods.includes(method))) return;
          try {
            const key = nip44.v2.utils.getConversationKey(this.secret, e.pubkey);
            const payload: unknown = JSON.parse(nip44.v2.decrypt(proof, key));
            if (!record(payload) || payload.challenge !== this.challenge) return;
            this.key = key;
          } catch { return; }
          this.wallet = e.pubkey;
          this.offered = methods;
          clearTimeout(this.expiryTimer);
          this.expiryTimer = setTimeout(() => this.close('The phone connection expired.'), (this.expires - now()) * 1000);
          this.stop.signal.removeEventListener('abort', abort);
          resolve();
        },
      });
      const query = new URLSearchParams({
        relay: PHONE_RELAY, state: this.state, name: 'Mosaik', request_methods: REQUIRED.join(' '),
        optional_request_methods: PREPARE, expires_at: String(this.expires), metadata: JSON.stringify({network: NETWORK, challenge: this.challenge}),
      });
      showQr(`nostr+walletauth://${this.pubkey}?${query.toString().replace(/\+/g, '%20')}`);
    });
  }

  private receive(event: Event) {
    const item = this.pending.get(field(event, 'e') ?? '');
    if (!item || event.kind !== RESPONSE || event.pubkey !== this.wallet || field(event, 'encryption') !== 'nip44_v2'
      || event.created_at < item.started - 30 || now() >= item.expires || !this.key || event.content.length > 100_000) return;
    try {
      const reply: unknown = JSON.parse(nip44.v2.decrypt(event.content, this.key));
      if (record(reply) && reply.result_type === item.method) item.reply(reply);
    } catch { /* Ignore malformed or unrelated relay events. */ }
  }

  private async request(method: string, args: unknown[] = []) {
    if (this.stop.signal.aborted) throw this.stop.signal.reason;
    if (now() >= this.expires) {
      this.close('The phone connection expired.');
      throw this.stop.signal.reason;
    }
    if (!this.wallet || !this.key) throw new Error('Reconnect the phone wallet.');
    if (method === PREPARE && [...this.pending.values()].some(item => item.method === PREPARE))
      throw new Error('A preparation request is waiting for your phone.');
    if (!this.relay.connected) throw new Error('Waiting for the Nostr relay to reconnect.');
    const started = now();
    const expires = Math.min(this.expires, started + 90);
    const event = finalizeEvent({
      kind: REQUEST, created_at: started,
      tags: [['p', this.wallet], ['encryption', 'nip44_v2'], ['expiration', String(expires)]],
      content: nip44.v2.encrypt(JSON.stringify({method, params: method === 'get_info' ? {} : {args}}), this.key),
    }, this.secret);
    let timer: ReturnType<typeof setTimeout> | undefined;
    let abort: (() => void) | undefined;
    try {
      return await new Promise<unknown>((resolve, reject) => {
        abort = () => reject(this.stop.signal.reason);
        this.stop.signal.addEventListener('abort', abort, {once: true});
        timer = setTimeout(() => this.close(method === PREPARE
          ? 'The phone did not confirm the outcome. Check transactions in Rate before preparing again.'
          : 'The phone did not reply. Open Rate and reconnect.'), (expires - started) * 1000);
        this.pending.set(event.id, {method, started, expires, reply: payload => {
          if (record(payload.error)) {
            if (payload.error.code === 'UNAUTHORIZED') this.close('Rate revoked this connection.');
            reject(new Error(typeof payload.error.message === 'string' ? payload.error.message : 'The phone declined the request.'));
          } else if (method === 'get_info') resolve(payload.result);
          else if (record(payload.result) && Object.hasOwn(payload.result, 'value')) resolve(payload.result.value);
          else reject(new Error('Invalid response from the phone wallet.'));
        }});
        void this.relay.publish(event).catch(() => {
          if (!this.pending.has(event.id)) return;
          if (method === PREPARE) this.close('Could not confirm delivery. Check transactions in Rate before preparing again.');
          else reject(new Error('The relay could not deliver the request.'));
        });
      });
    } finally {
      clearTimeout(timer);
      if (abort) this.stop.signal.removeEventListener('abort', abort);
      this.pending.delete(event.id);
    }
  }

  async open(showQr: (uri: string) => void): Promise<BrowserProvider> {
    try {
      await this.pair(showQr);
      const getInfo = async () => {
        const info = await this.request('get_info');
        const methods = record(info) && Array.isArray(info.methods) ? info.methods : [];
        if (!record(info) || info.network !== 'signet' || info.rgb_network !== NETWORK
          || !Array.isArray(info.extensions) || !info.extensions.includes('08')
          || !REQUIRED.every(method => methods.includes(method))) {
          this.close('Reconnect an RGB phone wallet on Mutinynet.');
          throw this.stop.signal.reason;
        }
        return {network: NETWORK, methods};
      };
      const info = await getInfo();
      const provider: BrowserProvider = {
        getInfo,
        getBtcBalance: async () => validateBalance(await this.request('kaleido_get_btc_balance')),
        listAssets: async () => {
          const assets = await this.request('kaleido_list_assets');
          if (Array.isArray(assets)) return assets;
          if (record(assets) && Array.isArray(assets.assets)) return assets.assets;
          throw new Error('Invalid RGB assets from the phone.');
        },
      };
      if (this.offered.includes(PREPARE) && info.methods.includes(PREPARE)) provider.createUtxos = async args => {
        const result = await this.request(PREPARE, [args]);
        if (!record(result) || typeof result.created !== 'number' || !Number.isSafeInteger(result.created) || result.created < 0)
          throw new Error('Check the allocation transaction in Rate.');
        return {created: result.created};
      };
      return provider;
    } catch (e) {
      this.close(e instanceof Error ? e.message : 'Could not connect to the phone wallet.');
      throw e;
    }
  }
}
