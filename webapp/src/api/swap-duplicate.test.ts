import { expect, it } from 'vitest';
import { failure, failureFromError } from './swapProblems';
it('does not claim nothing was spent when another tab already started the order', () => {
  const error = failureFromError(new Error('order_gone: this wallet already has a swap for this order'));
  expect(error.title).toBe('This order is already in progress');
  expect(error.todo).toContain('existing operation');
  expect(error.todo).not.toContain('Nothing was spent');
  expect(error.retry).toBe(false);
});

 it('does not offer to retry an expired order archived by the RGB wallet', () => {
  expect(failure('expired').retry).toBe(false);
  expect(failure('order_gone', 'this wallet already tried this order and it did not complete').title).toBe('Choose a fresh order');
});
