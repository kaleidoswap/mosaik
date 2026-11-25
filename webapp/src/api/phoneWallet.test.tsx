import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest';
import { EventEmitter } from 'node:events';

const testSdk = vi.hoisted(() => {
  vi.stubEnv('VITE_WALLETCONNECT_PROJECT_ID', 'a'.repeat(32));
  return { init: vi.fn() };
});
vi.mock('@walletconnect/sign-client', () => ({ SignClient: { init: testSdk.init } }));
import { WebRgbProvider, useWebRgb } from './webrgb';
import { WalletButton } from '../components/WalletButton';
import { WalletPanel } from '../components/WalletPanel';

const emitter = new EventEmitter();
const methods = ['enable', 'getInfo', 'listAssets', 'getBtcBalance', 'createUtxos'].map(name => `rgb_${name}`);
const session = { topic: 'phone', expiry: Math.floor(Date.now() / 1000) + 3600, peer: { metadata: { url: 'https://rate.example' } }, namespaces: { rgb: { methods, accounts: ['rgb:mutinynet:wallet'], events: [] } }, sessionProperties: { webrgb: 'webrgb:1' } };
const request = vi.fn();
const disconnect = vi.fn(async () => {});
const cancel = vi.fn(async () => {});
let approve: () => void;
const connect = vi.fn(async () => ({
  uri: 'wc:topic@2?symKey=test', approval: () => new Promise(resolve => { approve = () => resolve(session); }),
}));
function MarketCapability() {
  const w = useWebRgb();
  return <p>Peer-to-peer trading {w.canTake ? 'enabled' : 'unavailable'}</p>;
}
function page() {
  render(<WebRgbProvider><WalletButton /><WalletPanel /><MarketCapability /></WebRgbProvider>);
}
beforeAll(async () => { await Promise.all([import('@utexo/webrgb-walletconnect'), import('@utexo/webrgb')]); });
beforeEach(() => {
  testSdk.init.mockResolvedValue({
    session: { get: () => session }, request, connect, disconnect,
    on: emitter.on.bind(emitter), off: emitter.off.bind(emitter), core: { pairing: { disconnect: cancel } },
  });
  request.mockImplementation(async ({request: rpc}) => {
    switch (rpc.method) {
      case 'rgb_enable': return;
      case 'rgb_getInfo': return { ready: true, network: 'mutinynet', methods: methods.map(m => m.slice(4)) };
      case 'rgb_getBtcBalance': return { vanilla: { spendable: 50000, settled: 50000, future: 50000 }, colored: { settled: 0 }, freeColorableUtxos: 0 };
      case 'rgb_listAssets': return { assets: [{ assetId: 'rgb:abc', ticker: 'TST', precision: 2, balance: 1200 }] };
      case 'rgb_createUtxos': return { created: 2, size: 10000 };
      default: throw new Error('Unexpected RPC');
    }
  });
});
afterEach(() => { cleanup(); sessionStorage.clear(); vi.clearAllMocks(); emitter.removeAllListeners(); });

it('scans a Mutinynet session, reads balances, and requests one approved setup transaction from the Wallet page', async () => {
  page();
  fireEvent.click(screen.getByRole('button', {name: 'Connect phone'}));
  await screen.findByText(/Scan with Rate/);
  expect(connect).toHaveBeenCalledWith(expect.objectContaining({requiredNamespaces: {rgb: {chains: ['rgb:mutinynet'], methods: ['rgb_enable','rgb_getInfo','rgb_getBtcBalance','rgb_listAssets'], events: []}}}));
  await act(async () => approve());
  await screen.findByText('TST');
  expect(screen.getByText('Peer-to-peer trading unavailable')).toBeTruthy();
  expect(request.mock.calls.some(([rpc]) => rpc.request.method === 'rgb_createUtxos')).toBe(false);
  fireEvent.click(screen.getByRole('button', {name: 'Prepare wallet'}));
  await screen.findByText(/Preparation submitted/);
  expect(request.mock.calls.filter(([rpc]) => rpc.request.method === 'rgb_createUtxos')).toEqual([[
    { topic: 'phone', chainId: 'rgb:mutinynet', request: {method: 'rgb_createUtxos', params: [{num: 2, size: 10000, feeRate: 2}]} },
  ]]);
  act(() => { emitter.emit('session_delete', {topic: 'phone'}); });
  await screen.findByRole('button', {name: 'Connect phone'});
  expect(screen.queryByText('TST')).toBeNull();
});

it('cancels a QR and closes a session approved after the cancellation', async () => {
  page();
  fireEvent.click(screen.getByRole('button', {name: 'Connect phone'}));
  await screen.findByText(/Scan with Rate/);
  fireEvent.click(screen.getByRole('button', {name: 'Cancel'}));
  expect(cancel).toHaveBeenCalledWith({topic: 'topic'});
  await act(async () => approve());
  await waitFor(() => expect(disconnect).toHaveBeenCalledWith(expect.objectContaining({topic: 'phone'})));
  expect(request).not.toHaveBeenCalled();
});
