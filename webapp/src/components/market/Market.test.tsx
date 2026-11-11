import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import type { RgbBookState, RgbOffer } from '../../api/rgbp2p';
const state = vi.hoisted(() => ({ swaps: [] as any[] }));
vi.mock('../../api/webrgb', () => ({ useWebRgb: () => ({ status: 'absent', canTake: false, network: 'bitcoin' }) }));
vi.mock('../../api/swaps', () => ({ useSwaps: () => state, snapshotOf: (o: unknown) => o }));
vi.mock('../../api/registry', () => ({ useRegistry: () => ({ data: undefined }) }));
vi.mock('../TakeModal', () => ({ TakeModal: () => null }));
vi.mock('./Ladder', () => ({ Ladder: () => null }));
vi.mock('./Ticket', () => ({ Ticket: () => null }));
vi.mock('./MySwaps', () => ({ MySwaps: () => null }));
vi.mock('./PriceChart', () => ({ PriceChart: () => null }));
vi.mock('./TradeFeed', () => ({ TradeFeed: () => null }));
vi.mock('./TrustNotice', () => ({ TrustNotice: () => null }));
vi.mock('./PairBar', () => ({ PairBar: (p: {network: string; lastFill?: number}) => <div data-testid="pair" data-network={p.network} data-last={p.lastFill ?? ''} /> }));
import { Market } from './Market';
const offer = (network: string): RgbOffer => ({ key: network, orderId: network, maker: 'maker',
  side: 'sell', assetId: 'rgb:test', amount: 10, priceSat: 100, expiry: 9999999999,
  network, createdAt: 1, precision: 0, ticker: 'TEST' });
const book = (offers: RgbOffer[]): RgbBookState => ({ url: 'wss://example.test', status: 'open', offers, fills: [] });
afterEach(() => { cleanup(); state.swaps = []; });
it('does not expose mainnet offers even when the wallet is on mainnet', () => {
  render(<Market book={book([offer('bitcoin')])} />);
  expect(screen.queryByTestId('pair')).toBeNull();
  expect(screen.getByText('The next trade starts with an offer.')).toBeTruthy();
});
it('never uses another network’s settled trade as the Signet last price', () => {
  state.swaps = [{state: 'settled', updatedAt: 2, order: offer('bitcoin')}];
  render(<Market book={book([offer('signetcustom'), offer('bitcoin')])} />);
  expect(screen.getByTestId('pair').getAttribute('data-network')).toBe('signetcustom');
  expect(screen.getByTestId('pair').getAttribute('data-last')).toBe('');
});
it('uses a settled trade on the selected Signet network', () => {
  state.swaps = [{state: 'settled', updatedAt: 2, order: offer('signetcustom')}];
  render(<Market book={book([offer('signetcustom')])} />);
  expect(screen.getByTestId('pair').getAttribute('data-last')).toBe('10');
});
