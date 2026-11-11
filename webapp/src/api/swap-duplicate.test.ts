import { expect, it } from 'vitest';
import { failureFromError } from './swapProblems';
it('does not claim nothing was spent when another tab already started the order', () => {
  const error = failureFromError(new Error('order_gone: this wallet already has a swap for this order'));
  expect(error.title).toBe('This order is already in progress');
  expect(error.todo).toContain('existing operation');
  expect(error.todo).not.toContain('Nothing was spent');
  expect(error.retry).toBe(false);
});
