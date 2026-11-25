import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { finalizeEvent, generateSecretKey, getPublicKey, nip44, verifyEvent, type Event } from 'nostr-tools';
import { PhoneNwcSession, PHONE_RELAY, type PhoneRelay } from './nwc';
import { WebRgbProvider, useWebRgb } from './webrgb';
import { WalletButton } from '../components/WalletButton';
import { WalletPanel } from '../components/WalletPanel';

vi.mock('nostr-tools', async original => ({
  ...await original<typeof import('nostr-tools')>(),
  Relay: class { constructor(url: string) { expect(url).toBe(PHONE_RELAY); return relay; } },
}));
const methods = ['get_info','kaleido_get_btc_balance','kaleido_list_assets','kaleido_create_utxos'];
const wallet = generateSecretKey();
const time = () => Math.floor(Date.now() / 1000);
type Call = {method: string; params: {args?: unknown[]}};
let calls: Call[] = [];
let uri: URL;
let listener: (event: Event) => void;
let withhold = false;
let revoked = false;
let rgbNetwork = 'mutinynet';
const relay: PhoneRelay = {
  connected: true,
  connect: vi.fn(async () => {}),
  subscribe: vi.fn((_filters, opts) => { listener = opts.onevent; return {close() {}}; }),
  close: vi.fn(),
  publish: vi.fn(async (event: Event) => {
    const publicKey = uri.hostname;
    expect(verifyEvent(event)).toBe(true);
    expect(event.pubkey).toBe(publicKey);
    expect(event.kind).toBe(23194);
    expect(event.tags).toContainEqual(['p',getPublicKey(wallet)]);
    expect(event.tags).toContainEqual(['encryption','nip44_v2']);
    expect(Number(event.tags.find(t => t[0] === 'expiration')?.[1])).toBeGreaterThan(time());
    const key = nip44.v2.utils.getConversationKey(wallet, publicKey);
    const request = JSON.parse(nip44.v2.decrypt(event.content, key)) as Call;
    calls.push(request);
    if (withhold) return 'accepted';
    const results: Record<string, unknown> = {
      get_info: {alias:'Rate', network:'signet', rgb_network:rgbNetwork, methods, extensions:['08']},
      kaleido_get_btc_balance: {value:{vanilla:{spendable:50000,settled:50000,future:50000},colored:{settled:0},freeColorableUtxos:0}},
      kaleido_list_assets: {value:[{assetId:'rgb:abc',ticker:'TST',balance:1200,precision:2}]},
      kaleido_create_utxos: {value:{created:2,size:10000}},
    };
    const response = finalizeEvent({
      kind:23195, created_at:time(), tags:[['p',publicKey],['e',event.id],['encryption','nip44_v2']],
      content:nip44.v2.encrypt(JSON.stringify(revoked ? {result_type:request.method,error:{code:'UNAUTHORIZED',message:'revoked'}}
        : {result_type:request.method,result:results[request.method]}),key),
    },wallet);
    listener(response);
    return 'accepted';
  }),
};

function approval(challenge?: string) {
  const key = nip44.v2.utils.getConversationKey(wallet, uri.hostname);
  const {challenge: correct} = JSON.parse(uri.searchParams.get('metadata')!);
  return finalizeEvent({
    kind:13194, created_at:time(), content:methods.join(' '),
    tags:[['p',uri.hostname],['state',uri.searchParams.get('state')!],['encryption','nip44_v2'],['extensions','08'],
      ['proof',nip44.v2.encrypt(JSON.stringify({challenge:challenge ?? correct}),key)]],
  },wallet);
}

function Connection() {
  const w = useWebRgb();
  if (w.phone.uri) uri = new URL(w.phone.uri);
  return <p>Peer-to-peer trading {w.canTake ? 'enabled' : 'unavailable'}</p>;
}
function page() {
  render(<WebRgbProvider><WalletButton /><WalletPanel /><Connection /></WebRgbProvider>);
}
afterEach(() => {cleanup(); calls=[]; withhold=false; revoked=false; rgbNetwork='mutinynet'; vi.useRealTimers(); vi.clearAllMocks();});

it('pairs via NWA proof and prepares a Mutinynet allocation only after clicking Prepare wallet', async () => {
  page();
  fireEvent.click(screen.getByRole('button', {name:'Connect phone'}));
  await screen.findByText(/Scan with Rate/);
  expect(uri.protocol).toBe('nostr+walletauth:');
  expect(uri.searchParams.get('relay')).toBe(PHONE_RELAY);
  expect(uri.searchParams.get('state')).toMatch(/^[a-f0-9]{32}$/);
  expect(uri.searchParams.get('request_methods')).toBe(methods.slice(0, 3).join(' '));
  expect(uri.searchParams.get('optional_request_methods')).toBe('kaleido_create_utxos');
  const {network,challenge} = JSON.parse(uri.searchParams.get('metadata')!);
  expect(network).toBe('mutinynet');
  expect(challenge).toMatch(/^[a-f0-9]{32}$/);
  expect(challenge).not.toBe(uri.searchParams.get('state'));
  listener(approval('incorrect challenge'));
  const tampered = JSON.parse(JSON.stringify(approval()));
  tampered.sig = '0'.repeat(128);
  listener(tampered);
  expect(calls).toHaveLength(0);
  await act(async () => listener(approval()));
  await screen.findByText('TST');
  expect(screen.getByText('Peer-to-peer trading unavailable')).toBeTruthy();
  expect(calls.some(c=>c.method === 'kaleido_create_utxos')).toBe(false);
  fireEvent.click(screen.getByRole('button',{name:'Prepare wallet'}));
  await screen.findByText(/Preparation submitted/);
  expect(calls.filter(c=>c.method==='kaleido_create_utxos')).toEqual([
    {method:'kaleido_create_utxos',params:{args:[{num:2,size:10000,feeRate:2}]}},
  ]);
  revoked = true;
  fireEvent.click(screen.getByRole('button',{name:'Refresh wallet'}));
  await screen.findByRole('button',{name:'Connect phone'});
  expect(screen.queryByText('TST')).toBeNull();
});

it('drops late approvals and creates a fresh identity when pairing again', async () => {
  page();
  fireEvent.click(screen.getByRole('button',{name:'Connect phone'}));
  await screen.findByText(/Scan with Rate/);
  const previousKey=uri.hostname, oldEvent=approval(), oldListener=listener;
  fireEvent.click(screen.getByRole('button',{name:'Cancel'}));
  oldListener(oldEvent);
  fireEvent.click(screen.getByRole('button',{name:'Connect phone'}));
  await screen.findByText(/Scan with Rate/);
  expect(uri.hostname).not.toBe(previousKey);
  listener(oldEvent);
  expect(calls).toHaveLength(0);
  expect(relay.close).toHaveBeenCalled();
});

it('expires an unanswered request without resending the financial operation', async () => {
  vi.useFakeTimers();
  const session = new PhoneNwcSession(relay);
  const provider = await session.open(qr => {
    uri = new URL(qr);
    listener(approval());
  });
  withhold = true;
  const result = expect(provider.createUtxos!({num:2, size:10000, feeRate:2})).rejects.toThrow(/Check transactions/);
  await expect(provider.createUtxos!({num:2, size:10000, feeRate:2})).rejects.toThrow('waiting for your phone');
  await vi.advanceTimersByTimeAsync(90000);
  await result;
  expect(calls.filter(c=>c.method === 'kaleido_create_utxos')).toHaveLength(1);
  await expect(provider.getInfo!()).rejects.toThrow(/Check transactions/);
});

it('rejects public Signet even when the wallet supports every requested method', async () => {
  rgbNetwork = 'signet';
  const session = new PhoneNwcSession(relay);
  await expect(session.open(qr => {
    uri = new URL(qr);
    listener(approval());
  })).rejects.toThrow('Mutinynet');
  expect(calls.map(c => c.method)).toEqual(['get_info']);
});
