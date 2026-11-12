import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
const state=vi.hoisted(()=>({network:'signetcustom',sync:vi.fn().mockResolvedValue({refreshed:true}),create:vi.fn().mockResolvedValue({created:2}),refresh:vi.fn().mockResolvedValue(undefined)}));
vi.mock('../../api/webrgb',()=>({useWebRgb:()=>({status:'connected',network:state.network,balance:{spendable:100000,freeColorableUtxos:0},provider:{createUtxos:state.create,refresh:state.sync},refresh:state.refresh})}));
import { Onboarding } from './Onboarding';
afterEach(()=>{cleanup();state.network='signetcustom';vi.clearAllMocks();});
it('never prepares outputs on another network',()=>{
 state.network='bitcoin';render(<Onboarding />);
 expect(screen.queryByRole('button',{name:'Prepare wallet'})).toBeNull();
 expect(screen.getByText('Select RGB · Mutinynet in the extension')).toBeTruthy();expect(state.create).not.toHaveBeenCalled();
});
it('only requests preparation after an explicit click and explains pending confirmation',async()=>{
 render(<Onboarding />);expect(state.create).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Prepare wallet'}));
 await waitFor(()=>expect(screen.getByRole('status').textContent).toContain('Wait for confirmation'));
 expect(state.create).toHaveBeenCalledOnce();expect(state.refresh).toHaveBeenCalledOnce();
});

it('syncs the network before rereading balances on manual refresh',async()=>{
 render(<Onboarding />);expect(state.sync).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Refresh wallet'}));
 await waitFor(()=>expect(screen.getByRole('status').textContent).toContain('Wallet synced'));
 expect(state.sync).toHaveBeenCalledOnce();expect(state.refresh).toHaveBeenCalledOnce();
 expect(state.sync.mock.invocationCallOrder[0]).toBeLessThan(state.refresh.mock.invocationCallOrder[0]!);
});
