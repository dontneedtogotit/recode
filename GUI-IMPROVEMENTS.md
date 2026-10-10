# recode GUI — Complete Improvement Audit

Generated 2026-09-25. Every item has file:line evidence. Severity: P0 = broken/misleading, P1 = high-impact UX, P2 = polish. Effort: S (<1h), M (1–4h), L (multi-hour).

## A. Latent defects (P0 — fix first)

- A1. **Command palette claims keyboard support it doesn't have.** Footer says `↑↓ to move, ↵ to select` but no key handler exists for arrows/Enter; Tab also escapes the dialog (no focus trap). Fix: roving `selected` index, ArrowUp/Down/Enter/Escape on the input, `role="listbox"` + `aria-activedescendant`, focus trap. — `src/components/CommandPalette.tsx:81-111` (M)
- A2. **Palette is navigation-only.** `onRunGoal` prop is passed in and unused; the input placeholder promises "run commands" that don't exist. Fix: implement `run <goal>` as a palette action section, add actions (self-heal, clear console, copy console, toggle theme/inspector). — `CommandPalette.tsx:30,77` (M)
- A3. **Sidebar run-status is wrong whenever the inspector is closed.** It reads `rightPanel?.runState.status`, which is `undefined` → always "Ready", even mid-run. Fix: pass `runState` to AppShell as its own prop. — `src/components/AppShell.tsx:137-138` (S)
- A4. **Console drawer "Close" is a no-op in the runner view** (`onClose: () => {}`), so the drawer can never be dismissed. Fix: real state to hide it. — `src/App.tsx:122` (S)
- A5. **Cancel doesn't cancel.** `handleCancel` only flips local state to `failed`; the backend `run_pipeline` invoke keeps running and its result will later overwrite state via the `result` effect. Fix: backend cancel token/command, or remove the button until it works. — `src/App.tsx:85-91` (M, needs Rust)
- A6. **Console timestamps are fabricated and nothing is live.** `ts: Date.now() - (n - i) * 1000` invents a 1s-per-event timeline; entries only appear when the run *completes* because `useRunPipeline` is a single blocking invoke. Backend already supports `emit_events`. Fix: Tauri channel/event stream into `consoleEntries` with real `ts`; show steps as they happen. — `src/App.tsx:35-42`, `src/hooks/useRunPipeline.ts:27-44` (L, the biggest functional win)
- A7. **`execute_agent_task` is called with the wrong argument by 3 of 4 call sites.** Backend struct requires `agent_id` + `payload` (`src-tauri/src/agents.rs:16-19`), but:
  - `src/App.tsx:66` sends `{ id, payload }` (drives Diagnostics "Capture sample diagnostics", right-panel "Run sample task", console agent entries)
  - `src/hooks/useAgentRuntimeExecution.ts:14` sends `{ id, payload }` (drives Settings "Run test agent task")
  - `src/lib/ipc.ts:18-20` sends `{ id, payload }`
  Only `AgentChatView.tsx:60` uses `agent_id` correctly. Every other agent entry point fails with "missing field `agent_id`".
- A8. **Response field mismatch compounds A7.** Backend returns `task_id` (serde default snake_case, `agents.rs:23`); `useAgentRuntimeExecution.ts:17,27` reads `result.taskId` (camelCase) → always undefined → Settings test task always reports "missing task id in response" even after A7 is fixed. Normalize in one typed IPC layer.
- A9. **Digit shortcuts don't match nav order and the palette's own list disagrees.** Nav: project=1, agent=2, runner=3, plans=5, diagnostics=4, telemetry=6, settings=7, updates=8, history=9, ai=0, learning=L. Diagnostics/Plans are swapped, history=9 is out of band. Fix: renumber sequentially in DOM order in both `AppShell.tsx NAV_ITEMS` and `CommandPalette.tsx SECTIONS`, single source of truth. — `AppShell.tsx:25-37`, `CommandPalette.tsx:11-28` (S)
- A10. **Dev artifact in the production sidebar.** Hardcoded badge "Direct dev path active". — `AppShell.tsx:140` (S)
- A11. **Dead Ctrl+K handler.** AppShell intercepts Ctrl+K with `preventDefault` and does nothing (CommandPalette has its own listener) — dead code that will confuse future debugging. — `AppShell.tsx:63-67` (S)
- A12. **Agent chat relabels history when you switch agents.** Message header renders the *current* `agentId` state, so all past messages get re-attributed. Fix: store `agentId` per message. — `src/views/AgentChatView.tsx:104` (S)
- A13. **Settings number inputs persist NaN.** `Number(e.target.value)` on an emptied field stores `NaN` into runtime settings and saves it. Fix: guard empty/NaN, clamp min/max, disable save on invalid. — `src/views/SettingsView.tsx:108-115` (S)
- A14. **Accent picker is partially fake.** Theme sets `--accent*` vars, but ~12 CSS rules hardcode violet, so choosing Emerald/Sky/Amber/Rose leaves primary buttons, active nav, active panel tabs, user chat bubbles, app background glow, and shadow-glow purple:
  - `global.css:31` `--shadow-glow`, `:63` app-shell radial, `:106` brand, `:156-157` `.nav-item.active`, `:169`, `:402` primary button gradient, `:597-599` `.chat-bubble.user`, `:675` `.right-panel-tab.active`
  Fix: emit `--accent-rgb` from `theme.tsx` and replace hardcoded values with `rgb(var(--accent-rgb) / α)` (or `color-mix(in srgb, var(--accent) α%, transparent)`).
- A15. **No frontend test setup despite tooling installed.** `vitest`, `@testing-library/react`, `jsdom` are devDependencies but there are zero `*.test.*` files. Fix: add the 3 config bits + a smoke test per view to lock in A1–A14 fixes. (S for setup, then ongoing)

## B. Live feedback & streaming (P1)

- B1. Stream pipeline events (A6) — the umbrella fix.
- B2. **Plan step progression UI.** The run result already contains `plan.steps[]` with per-step status and `repair_actions[]`; render a checklist with spinners/checks/x and applied-vs-not styling. — `src/hooks/useRunPipeline.ts:10-18` (M)
- B3. **Elapsed-time ticker + per-step durations** while running. (S)
- B4. **Runner console has no auto-scroll.** Entries render but the view doesn't follow; add stick-to-bottom that pauses when the user scrolls up, with a "↓ new output" pill to re-enable. — `src/views/RunnerView.tsx:49-72` (S)
- B5. **Agent chat has no streaming and no stop.** Just a "…" bubble until the whole task returns. Add token streaming (backend channel) + Stop button. (L)
- B6. **Toasts for run completed/failed** so you can be on any view; plus Tauri OS-level notification option. (M)
- B7. **Window title reflects view + run state** via `window.__TAURI__.window.setTitle` — helps when the window is in a workspace tab. (S)
- B8. **Diagnostics events only reload when `runState.status` changes** — a `useEffect([runState.status])` is not a monitor. Poll every 3–5s or subscribe to events; add severity filter + clear. — `src/views/DiagnosticsView.tsx:16-18` (S)

## C. Theme & visual system (P1/P2)

- C1. **Light + system theme.** Dark-only today; add `light`/`system` (prefers-color-scheme) with the same accent mechanism, persisted like accent. (M)
- C2. **Remove the remote Google Fonts `@import`** (line 1) — desktop app: bundle Inter via `@fontsource` or drop to the existing system stack. Blocks first paint and fails offline. (S)
- C3. **Density / font-size setting** (compact/comfortable) — one CSS var switch, high value in a dev tool. (S)
- C4. **Accent picker: swatch dots instead of text pills**, and make it actually work (A14). (S)
- C5. **Custom scrollbars** for console/chat/history scroll regions — matches the dark surface. (S)
- C6. **Contrast audit.** `--text-muted: #5f6672` at 11–12px on `#0b0d12` is under WCAG AA (≈4.0:1). Raise to ~#7d8595. Check `.nav-item-shortcut`, `.palette-section-title`, `.metric-label`. (S)
- C7. **Focus-visible rings for buttons/nav.** All buttons are `outline: none` with no fallback; keyboard users have no focus indicator outside inputs. Add `:focus-visible { box-shadow: 0 0 0 3px var(--accent-soft) }` to `button`, `.nav-item`, `.palette-item`. — `global.css:393-446` (S)
- C8. **Dead CSS/components to prune:** `.badge.info` rule duplicated verbatim (`global.css:486-494`); `--console-height` defined but never applied to the drawer (`:36,:836`); `.card-ghost` unused; `Pill.tsx` component unused; `EmptyState.action` prop never passed anywhere. (S)
- C9. Icons are already consistent (NavIcons wraps lucide) — no change needed.

## D. Keyboard & navigation (P1)

- D1. Palette keyboard nav + focus trap (A1) and actions (A2).
- D2. **Cmd/Ctrl+Enter runs the goal** from Project; goal input becomes a `textarea` (Enter=run, Shift+Enter=newline) with a recent-goals `<datalist>`. — `src/views/ProjectView.tsx:156-166` (S)
- D3. **Sidebar arrow-key navigation** + Home/End; renumber digits (A9); add a "Shortcuts" entry in the palette that lists everything. (M)
- D4. **Global console toggle** (`` ` `` or Ctrl+) — console is currently only reachable from the runner view. (M)
- D5. Chat: multiline textarea with Shift+Enter newline (Enter stays send). (S)

## E. State persistence (P1 — your localStorage standard)

- E1. **Remember last view** (app always boots to Project). (S)
- E2. **Remember sidebar open state + right-panel open state + active tab.** (S)
- E3. **Remember workspace root + selected workspace**; also debounce the `list_workspaces` fetch (currently fires on every keystroke, no loading indicator, error only surfaces after failure). — `src/views/ProjectView.tsx:28-50` (S)
- E4. **Wire chat history to the existing `lib/agentHistory.ts`** — the store exists with load/save but `AgentChatView` never uses it; history dies on every view switch. (S)
- E5. **Recent goals list** (Project view quick-run + palette section). (S)
- E6. **Persist IPC benchmark results** across unmount (currently lost when you leave Settings). (S)

## F. Per-view improvements (P1/P2)

Project
- F1. Replace the "Project context placeholder" with real content (recent files, active plan, quick actions) or delete the card. There are **7 placeholder blocks app-wide** (Project, Runner, Agent chat ×2, Settings ×2, Updates, History, right-panel ×2) — each one erodes trust in an otherwise functional UI. (M–L each; at minimum convert to "coming soon" chips or remove)
- F2. Workspace select: show relative path, sort by recency. (S)

Runner
- F3. Console toolbar: filter by level/source, Clear, Copy all, Export JSON, auto-scroll toggle. (M)
- F4. On completion show **repair actions + per-step results** (data already in the result, see B2) and a run summary line (duration, steps, error count). (S)
- F5. Diff preview area for plan output (`.diff-placeholder` style already exists and is used by AIView). (M)

Agent chat
- F6. **Markdown rendering** for assistant messages + code blocks with copy buttons. (M)
- F7. Tool-call visualization as collapsed expandable blocks (planned in the view's own placeholder text). (M)
- F8. Per-message copy + "re-run this prompt" action. (S)
- F9. Agent switcher with capability chips per agent, not just a bare name `<select>`. (S)

Plans
- F10. Search/filter, relative timestamps, execute-from-list button, delete with confirm, per-step status badges. — `src/views/PlansView.tsx` (M)

Diagnostics
- F11. Severity filter, clear events, auto-refresh (B8), applied/not-applied repair styling. (S)

History
- F12. Search/filter, **re-run button**, bulk delete, **export JSON/CSV** (your standard export bar), success-rate summary in the header, relative timestamps. — `src/views/RunHistoryView.tsx` (M)

Telemetry / right-panel
- F13. Replace right-panel telemetry placeholder with a real IPC-latency sparkline (benchmark primitive already exists). (M)
- F14. Persist benchmark history and show a comparison strip. (S)

Learning
- F15. Score bars instead of raw numbers, success/anti color coding, delete-pattern button. (S)

Settings
- F16. Section nav (tabs or anchor sidebar) — it's one long scroll of 4 cards. (S)
- F17. **API keys: `type="password"` + reveal toggle** — plaintext `sk-...` in a text input. — `src/views/SettingsView.tsx:181` (S)
- F18. Confirm before delete provider; edit mode; per-provider "test connection". (M)
- F19. Replace hardcoded `https://example.com/updater` with a real, configurable endpoint. — `SettingsView.tsx:65` (S)
- F20. Danger zone: reset settings, clear run history; export/import settings JSON. (M)

Updates
- F21. Last-checked timestamp + remove the staged-rollout placeholder until the backend exists. (S)

AI view
- F22. Raw gateway response: pretty-print JSON/code view with copy button instead of a plain pre-block. (S)

## G. Accessibility & robustness (P1)

- G1. **ErrorBoundary** — no view crash isolation today; one throw white-screens the whole app. Add a top-level boundary with a reload button + error text. (S)
- G2. Focus-visible rings (C7).
- G3. `role="log"` + `aria-live="polite"` on console bodies so screen readers track output. (S)
- G4. Modal focus management: trap Tab in palette (A1) and restore focus to the invoking element on close. (S)
- G5. Distinguish "IPC failed" from "no data" — today both render identical empty states, which makes backend bugs invisible in the UI. Add a dev error banner. (S)
- (Already good and keep: `prefers-reduced-motion` gate, `aria-current` nav, `aria-live` main, empty-state components, `view-enter` animation.)

## H. Performance & architecture (P1)

- H1. **Views unmount on every navigation** (`App.tsx:153-180`) — typed goals, scroll positions, and loaded data are lost. Either lift shared state (goal, workspace, console) into App or keep-alive the four main views. (M)
- H2. **Virtualize long lists** once streaming lands (console, history, chat). The `slice(-300)` in RunnerView is a patch. (M)
- H3. **Typed IPC layer.** Raw `(window as any).__TAURI__.invoke` is duplicated in 8+ files with three different argument conventions (see A7/A8 — that's how it broke). Centralize in `src/lib/ipc.ts` with one typed wrapper per command; this makes A7/A8 impossible to reintroduce. (M)
- H4. Single settings store (accent + theme + density + panel state) on the existing localStorage pattern in `theme.tsx`. (S)

## I. Power features (P2 — later phases)

- I1. Drag-resize right panel (fixed 320px) and console drawer (fixed 240px). (S)
- I2. Diff viewer component (unified/split) — core to a code tool; B2/F5 feed it. (L)
- I3. File tree / git status panel in Project (`git.rs` already tracks staged/unstaged). (L)
- I4. Bulk select + actions in History/Plans; drag a plan onto Runner to execute. (M)
- I5. Palette as universal entry: `run <goal>` executes, `agent <text>` opens chat pre-filled. (M)
- I6. Multi-workspace switcher in the brand bar. (M)

## Recommended order

1. **P0 sweep (≈1 day, all small, zero risk):** A1–A15.
   Highest-leverage single fix: A7+A8 (one typed call site) unblocks three dead buttons.
   Highest-leverage feature: A6 (event streaming) turns "runner" from a fake live view into a real one.
2. **Streaming & feedback:** B2–B7, F3, F4.
3. **Persistence & keyboard:** E1–E6, D2–D5, C7.
4. **Theme correctness:** C1, C2, C4 (A14), C5–C8.
5. **Per-view power:** F6–F22 as backend bridges land.
6. **Architecture:** H1–H4 before the list views grow (H3 should land in step 1 alongside A7/A8).
7. **Power features:** I1–I6.

## Items already done right (do not regress)

- `prefers-reduced-motion` media gate on all animations
- `aria-current="page"` on nav, `aria-pressed` on panel tabs, `role="dialog" aria-modal` on palette
- EmptyState component used consistently for "no data"
- Theme accent persisted to localStorage with SSR-safe guard
- `agentHistory.ts` pure-state reducer pattern (ready to wire, see E4)
- Responsive shell (980px/640px breakpoints) with mobile sidebar drawer
