import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import CommandPalette from './CommandPalette';

describe('CommandPalette', () => {
  it('does not run commands while confirming composed text', () => {
    const onRunGoal = vi.fn();
    render(<CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={onRunGoal} open />);
    const input = screen.getByRole('combobox');
    fireEvent.change(input, { target: { value: 'run Build dashboard' } });
    fireEvent.keyDown(input, { key: 'Enter', isComposing: true });
    expect(onRunGoal).not.toHaveBeenCalled();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('focuses the input and restores the trigger on close', () => {
    render(<><button>Commands</button><CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={vi.fn()} /></>);
    const trigger = screen.getByRole('button', { name: 'Commands' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'k', ctrlKey: true });
    expect(screen.getByRole('combobox')).toHaveFocus();
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Escape' });
    expect(trigger).toHaveFocus();
  });

  it('opens with the keyboard shortcut and selects a view with arrow keys and Enter', () => {
    const onNavigate = vi.fn();
    render(<CommandPalette view="project" onNavigate={onNavigate} onRunGoal={vi.fn()} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    const input = screen.getByRole('combobox');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onNavigate).toHaveBeenCalledWith('agent');
  });

  it('runs a goal entered with the run prefix', () => {
    const onRunGoal = vi.fn();
    render(<CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={onRunGoal} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    const input = screen.getByRole('combobox');
    fireEvent.change(input, { target: { value: 'run Build the dashboard' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onRunGoal).toHaveBeenCalledWith('Build the dashboard');
  });

  it('invokes the selected action callback', () => {
    const onAction = vi.fn();
    render(<CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={vi.fn()} onAction={onAction} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    const input = screen.getByRole('combobox');
    fireEvent.change(input, { target: { value: 'clear console' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onAction).toHaveBeenCalledWith('clear-console');
  });

  it('keeps tab focus inside the palette after an option is focused', () => {
    render(<CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={vi.fn()} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    const input = screen.getByRole('combobox');
    const option = screen.getAllByRole('option')[0];
    option.focus();
    fireEvent.keyDown(option, { key: 'Tab' });
    expect(input).toHaveFocus();
  });

  it('closes on Escape', () => {
    render(<CommandPalette view="project" onNavigate={vi.fn()} onRunGoal={vi.fn()} />);
    fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
    expect(screen.getByRole('dialog')).toBeInTheDocument();
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
