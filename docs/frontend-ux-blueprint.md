# recode frontend UX blueprint

This is the UX and architecture target for the recode desktop frontend. It is designed to match the functional ambition of CodeForge—project-centric workflows, agent chat, run/console output, self-heal, plans, and telemetry—while keeping the desktop shell cleaner and more grounded in interaction design principles.

## design principles
- **progressive disclosure:** show only the primary action and current state; reveal history, diagnostics, and advanced controls on demand.
- **recognition over recall:** persistent project context, visible run state, and obvious affordances instead of hidden commands.
- **feedback immediacy:** every agent or runner action surfaces status within 100–300ms, then detail.
- **error recovery first:** failures are visible, explainable, and actionable, not silent.
- **consistent spatial memory:** fixed left nav, stable main context, optional right inspector, and a bottom console that behaves like a real terminal.

## target layout
- **left rail:** global navigation + project selector + run status.
- **main stage:** context-sensitive view with strong hierarchy and scannable status.
- **right panel:** inspector/chat/details, collapsible, not always visible.
- **bottom panel:** console/output with live tail, status, and actions.

## view map
- **project**
  - project selector, recent list, status summary, quick actions.
  - primary entrypoint instead of manual root entry everywhere.
- **agent chat**
  - full conversational interface with structured agent messages.
  - support for tool calls, diff proposals, apply/reject actions.
- **runner**
  - live console output, run controls, restart, stop, environment selector.
- **diagnostics**
  - monitor events, self-heal actions, failure grouping, retry.
- **plans**
  - plan list, step status, execution history, retry/abort.
- **telemetry**
  - IPC benchmark history, startup benchmarks, runtime metrics.
- **settings**
  - agent runtime, providers, updates, signature verification.
- **updates**
  - update checks, manifest info, fallback behavior.

## interaction targets
- primary buttons: min 44px height on touch-capable paths.
- nav targets: large enough for fast selection, clear active state.
- console text: monospaced, selectable, supports copy.
- status: color + icon + short text, never color alone.

## implementation order
1. shell and layout primitives
2. project + runner + console
3. agent chat
4. diagnostics/plans/telemetry/settings/updates
5. polish: shortcuts, empty states, motion, accessibility pass
