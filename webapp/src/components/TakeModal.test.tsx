import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { OrderSnapshot, SwapRecord } from '../api/swaps';
const state = vi.hoisted(() => ({ swaps: [] as SwapRecord[] }));
vi.mock('../api/swaps', async (original) => ({ ...await original<typeof import('../api/swaps')>(), useSwaps: () => ({swaps:state.swaps}) }));
vi.mock('../api/webrgb', () => ({useWebRgb:()=>({status:'connected',canTake:true,network:'signetcustom',assets:[],balance:{spendable:100000,freeColorableUtxos:2}})}));
import { TakeModal } from './TakeModal';
afterEach(cleanup);
it('explains signed recovery without presenting cancellation or implying broadcast', () => {
 const order:OrderSnapshot={key:'maker:1',orderId:'1',maker:'maker',side:'buy',assetId:'rgb:testasset',amount:50,priceSat:49000,expiry:1,network:'signetcustom',ticker:'TESTUSD',precision:0};
 state.swaps=[{swapId:'1',order,state:'signing',startedAt:Date.now()-180000,updatedAt:Date.now(),recovery:{action:'resume_same_swap',canCancel:false,candidateTxid:'a'.repeat(64)}}];
 render(<TakeModal order={order} swapId="1" onClose={()=>{}} />);
 expect(screen.getByText('This swap needs recovery')).toBeTruthy();
 expect(screen.getByText(/Candidate transaction \(not proof of broadcast\)/)).toBeTruthy();
 expect(screen.queryByRole('button',{name:/cancel|refund|retry trade/i})).toBeNull();
 expect(screen.queryByRole('link',{name:'View on explorer'})).toBeNull();
});
