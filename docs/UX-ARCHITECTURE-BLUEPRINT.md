# recode GUI redesign blueprint

Date: 2026-09-25
Scope: polish and complete the existing Tauri/React shell without replacing the working IPC boundary.

## Experience model

`Project` is the command surface: choose a workspace, write a goal, and start a run. `Runner` is the operational surface: observe the active plan, live events, repairs, and completion state. `Agent` is a focused conversation surface. `Inspect` groups diagnostics, plans, and telemetry; `System` groups history, settings, and updates.

The shell remains one layout model:

- persistent intent-grouped sidebar;
- compact top bar with current context, run state, and command-palette entry point;
- main content surface;
- optional inspector;
- optional console drawer.

## Visual system

Use semantic CSS variables for surfaces, borders, text, status, accent, density, and theme. Dark is the default; light follows the same component structure. No engineering notes or placeholder blocks are shown as product content. Empty states explain what is missing and offer the next useful action where one exists.

## Interaction contract

- Ctrl/Cmd+K opens the command palette; arrows, Enter, Escape, and focus restoration work.
- `1`–`9`, `0`, and `L` navigate using the same navigation map shown in the shell.
- ` or Ctrl/Cmd+` toggles the runner console from any view.
- Cmd/Ctrl+Enter runs a project goal; Enter sends a chat message; Shift+Enter inserts a newline.
- Live pipeline events use real backend timestamps and are de-duplicated before reaching the console.
- Clearing, exporting, copying, rerunning, self-healing, inspector toggling, and update/signature actions all use real handlers.

## View ownership

- `App.tsx`: navigation, run state, console aggregation, and cross-view actions only.
- `AppShell.tsx`: chrome, navigation, inspector, console, and appearance controls.
- Views: local form/filter/history state and backend hooks.
- `lib/ipc.ts`: the only frontend command boundary and wire-shape normalizer.
- `global.css`: responsive layout, tokens, component states, and accessibility affordances.

## Verification order

1. shell and theme smoke tests;
2. runner event de-duplication and clear/rerun behavior;
3. changed-view interaction tests;
4. type-check and build;
5. report any unrelated baseline failures without widening the change.
