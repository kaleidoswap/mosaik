import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { candlesOf } from './stats';
import { PriceChart } from './PriceChart';
afterEach(cleanup);
it('aggregates chronologically and leaves missing intervals empty',()=>{
 expect(candlesOf([{at:110,price:12,units:1},{at:5,price:10,units:1},{at:20,price:8,units:1},{at:50,price:11,units:1},{at:310,price:9,units:1}],100)).toEqual([
 {at:0,open:10,high:11,low:8,close:11,trades:3},{at:100,open:12,high:12,low:12,close:12,trades:1},{at:300,open:9,high:9,low:9,close:9,trades:1}]);
});
it('never invents candles when no trades exist',()=>{
 render(<PriceChart points={[]} ticker="TESTUSD"/>);
 expect(screen.getByText('No trades reported yet')).toBeTruthy();
 expect(screen.queryByRole('img')).toBeNull();
});
it('switches styles and intervals for actual reported trades',()=>{
 render(<PriceChart points={[{at:3600,price:10,units:1},{at:3660,price:12,units:1}]} ticker="TESTUSD"/>);
 expect(screen.getByRole('img').getAttribute('aria-label')).toContain('Candlestick');
 fireEvent.change(screen.getByLabelText('Candle interval'),{target:{value:'900'}});
 fireEvent.click(screen.getByRole('button',{name:'Line'}));
 expect(screen.getByRole('img').getAttribute('aria-label')).toContain('Price line');
});
