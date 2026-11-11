import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
const mock = vi.hoisted(() => ({ take: vi.fn(), status: vi.fn(), onStatus: vi.fn(() => () => {}), enable: vi.fn(), refresh: vi.fn() }));
vi.mock('./webrgb', () => ({ useWebRgb: () => ({ status: 'connected', provider: { enable: mock.enable, p2pSwap: mock }, refresh: mock.refresh, assets: [], balance: { spendable: 100000 } }) }));
import { SwapsProvider, useSwaps, type OrderSnapshot } from './swaps';
const wrapper = ({children}: {children: ReactNode}) => <SwapsProvider>{children}</SwapsProvider>;
const order: OrderSnapshot = { key:'maker:order', orderId:'order', maker:'maker', side:'sell', assetId:'rgb:test', amount:1, priceSat:1000, expiry:2_000_000_000, network:'signetcustom', precision:0 };
beforeEach(() => { localStorage.clear(); vi.clearAllMocks(); mock.status.mockResolvedValue(undefined); mock.enable.mockResolvedValue(undefined); });
afterEach(cleanup);
it('coalesces rapid repeated clicks into one wallet request', async () => {
 let accept!: (v: {swapId:string}) => void;
 mock.take.mockImplementation(() => new Promise(resolve => { accept = resolve; }));
 const {result} = renderHook(useSwaps,{wrapper});
 let a!: ReturnType<typeof result.current.start>, b!: typeof a;
 act(() => { a=result.current.start(order); b=result.current.start(order); });
 await waitFor(() => expect(mock.take).toHaveBeenCalledOnce());
 await act(async () => { accept({swapId:'s'}); await Promise.all([a,b]); });
 expect(result.current.swaps).toHaveLength(1);expect(mock.enable).toHaveBeenCalledOnce();
});
it('rejects expired orders before asking the wallet', async () => {
 const {result}=renderHook(useSwaps,{wrapper});
 await expect(result.current.start({...order,expiry:1})).rejects.toThrow('expired');
 expect(mock.take).not.toHaveBeenCalled();expect(mock.enable).not.toHaveBeenCalled();
});
it('recovers a persisted broadcast after reload without starting a second swap', async () => {
 localStorage.setItem('mosaik.p2p.swaps.v1',JSON.stringify([{swapId:'s',order,state:'broadcast',startedAt:1,updatedAt:1,txid:'tx'}]));
 mock.status.mockResolvedValue({swapId:'s',state:'settled',txid:'tx'});
 const {result}=renderHook(useSwaps,{wrapper});
 await waitFor(()=>expect(result.current.swaps[0]?.state).toBe('settled'));
 await act(async()=>{await result.current.start(order);});expect(mock.take).not.toHaveBeenCalled();
});
it('keeps a broadcast pending when status is temporarily unavailable', async () => {
 localStorage.setItem('mosaik.p2p.swaps.v1',JSON.stringify([{swapId:'s',order,state:'broadcast',startedAt:1,updatedAt:1,txid:'tx'}]));
 mock.status.mockRejectedValue(new Error('offline'));
 const {result}=renderHook(useSwaps,{wrapper});
 await waitFor(()=>expect(mock.status).toHaveBeenCalled());
 expect(result.current.swaps[0]?.state).toBe('broadcast');
 await act(async()=>{await result.current.start(order);});expect(mock.take).not.toHaveBeenCalled();
});
