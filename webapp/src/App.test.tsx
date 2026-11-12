import { act, cleanup, render, screen, fireEvent } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { ReactNode } from 'react';
const wallet = vi.hoisted(() => ({ status: 'ready', connect: vi.fn(), error: undefined as string | undefined }));
const research = vi.hoisted(() => ({ mounted: vi.fn() }));
vi.mock('./api/rgbp2p', () => ({ useRgbP2pBook: () => ({ status: 'open', orders: [], fills: [] }) }));
vi.mock('./api/webrgb', () => ({ WebRgbProvider: ({children}: {children: ReactNode}) => children, useWebRgb: () => wallet }));
vi.mock('./api/swaps', () => ({ SwapsProvider: ({children}: {children: ReactNode}) => children }));
vi.mock('./components/market/Market', () => ({ Market: () => <div>Signet market</div> }));
vi.mock('./components/market/MySwaps', () => ({ MySwaps: () => <div>Trade history</div> }));
vi.mock('./components/TakeModal', () => ({ TakeModal: () => null }));
vi.mock('./components/WalletButton', () => ({ WalletButton: () => <button>Connect wallet</button> }));
vi.mock('./components/WalletPanel', () => ({ WalletPanel: () => <div>Wallet balances</div> }));
vi.mock('./components/CovenantWorkspace', () => ({ default: () => { research.mounted(); return <div>Research trading</div>; } }));
import { App } from './App';
afterEach(() => { cleanup(); window.history.replaceState(null, '', '/'); vi.clearAllMocks(); wallet.status = 'ready'; wallet.error = undefined; });
it('keeps Covenant disabled and unmounted in the Signet preview, including the Labs deep link', () => {
  window.history.replaceState(null, '', '#labs');
  render(<App />);
  expect(screen.getByText('Covenant DEX / Tessera')).toBeTruthy();
  expect(screen.getByText('Arkade DEX / Banco')).toBeTruthy();
  expect(screen.getAllByText('Integration pending · not enabled here')).toHaveLength(2);
  expect(screen.queryByRole('button', {name: /Open .* research workspace/})).toBeNull();
  expect(research.mounted).not.toHaveBeenCalled();
});
it('responds to history navigation and explains full-order approval', () => {
  render(<App />);
  act(() => { window.history.replaceState(null, '', '#learn'); window.dispatchEvent(new HashChangeEvent('hashchange')); });
  expect(screen.getByText(/You take the whole offer, not part of it/)).toBeTruthy();
  expect(screen.queryByText('Signet market')).toBeNull();
  act(() => { window.history.replaceState(null, '', '#activity'); window.dispatchEvent(new HashChangeEvent('hashchange')); });
  expect(screen.getByText('Trade history')).toBeTruthy();
});
it('skip link focuses content without navigating away from the current view', () => {
  window.history.replaceState(null, '', '#learn');
  render(<App />);
  fireEvent.click(screen.getByText('Skip to content'));
  expect(document.activeElement?.id).toBe('main');
  expect(window.location.hash).toBe('#learn');
});

it('starts with the welcome page, without mounting the market', () => {
  render(<App />);
  expect(screen.getByRole('heading', {name: /Your wallet/})).toBeTruthy();
  expect(screen.queryByText('Signet market')).toBeNull();
  fireEvent.click(screen.getByRole('button', {name: /Enter Mosaik/}));
  expect(window.location.hash).toBe('#trade');
  act(() => window.dispatchEvent(new HashChangeEvent('hashchange')));
  expect(screen.getByText('Signet market')).toBeTruthy();
  expect(wallet.connect).not.toHaveBeenCalled();
});
it('lets users browse the market without a wallet setup gate', () => {
  window.history.replaceState(null, '', '#trade');
  render(<App />);
  expect(screen.getByText('Signet market')).toBeTruthy();
  expect(screen.getByRole('button', {name: 'Connect wallet'})).toBeTruthy();
  expect(wallet.connect).not.toHaveBeenCalled();
});
