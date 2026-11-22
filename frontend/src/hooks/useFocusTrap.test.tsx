import React from 'react';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { useFocusTrap, useEscapeClose } from './useFocusTrap';

function TrapHarness({ isOpen, onClose }: { isOpen: boolean; onClose: () => void }) {
  const ref = useFocusTrap(isOpen);
  useEscapeClose(isOpen, onClose);
  if (!isOpen) return <button type="button">outside</button>;
  return (
    <div>
      <button type="button">outside</button>
      <div ref={ref} data-testid="dialog">
        <button type="button">first</button>
        <button type="button">last</button>
      </div>
    </div>
  );
}

describe('useFocusTrap/useEscapeClose', () => {
  it('focuses first element and traps tab', async () => {
    const user = userEvent.setup();
    render(<TrapHarness isOpen onClose={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'first' })).toHaveFocus();

    await user.tab();
    expect(screen.getByRole('button', { name: 'last' })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole('button', { name: 'first' })).toHaveFocus();

    await user.tab({ shift: true });
    expect(screen.getByRole('button', { name: 'last' })).toHaveFocus();
  });

  it('calls onClose on Escape and restores focus on close', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const { rerender } = render(
      <div>
        <button type="button">trigger</button>
        <TrapHarness isOpen={false} onClose={onClose} />
      </div>,
    );

    screen.getByRole('button', { name: 'trigger' }).focus();
    rerender(
      <div>
        <button type="button">trigger</button>
        <TrapHarness isOpen onClose={onClose} />
      </div>,
    );
    expect(screen.getByRole('button', { name: 'first' })).toHaveFocus();

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(
      <div>
        <button type="button">trigger</button>
        <TrapHarness isOpen={false} onClose={onClose} />
      </div>,
    );
    expect(screen.getByRole('button', { name: 'trigger' })).toHaveFocus();
  });
});
