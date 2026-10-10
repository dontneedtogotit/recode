# Recode: from prototype to development workbench

## Product direction

Recode is an AI-assisted development workbench: managed workspaces, goal-driven generation, plan simulation, specialized agents, gateway requests, execution evidence, learning, and runtime maintenance. Its UI should optimize for making a brief, understanding what will execute, and reviewing what actually happened—not for filling a generic dashboard with decorative statistics.

The existing routes and IPC contracts remain intact. This implementation evolves their presentation rather than replacing the application. No dependencies were added. The earlier GUI-IMPROVEMENTS.md is a historical audit, not a statement of current defects.

### Information architecture

- Build: Project, Agent, Runner, Plans.
- Observe: Diagnostics, Telemetry, History, Learning.
- Configure: Settings, Updates, AI gateway.
- Global tools: command search, console, inspector. Appearance is secondary, in a disclosure rather than permanently occupying a large sidebar region.
- Project: brief and starter prompts on the primary side, managed workspace and execution context on the secondary side.
- Runner: state and measured progress first, files/outcomes/verification next, searchable event output with filtering and export.
- Responsive behavior follows the available content width, including space taken by the inspector—not only the browser width.

## Implemented idea inventory

“Implemented” means source changes are present and included in the successful production build. It does NOT mean interaction tests or screenshots have passed; those checks were denied.

Priority: P0 = correctness/trust, P1 = core workflow, P2 = polish. Effort: S = localized, M = cross-component. References point to the implementation, not an obsolete baseline.

| ID | Priority / effort | Idea and reason | Implementation evidence |
| --- | --- | --- | --- |
| I01 | P1 / M | Edge-to-edge application chrome instead of floating, glowing boxes; use screen space for work. | src/styles/global.css:76 |
| I02 | P1 / S | Group navigation by Build / Observe / Configure rather than implementation-oriented buckets. Preserve established key bindings. | src/components/AppShell.tsx:65 |
| I03 | P2 / S | Consistent code-mark identity and a restrained workbench subtitle. | src/components/AppShell.tsx:164 |
| I04 | P1 / S | Align navigation icons, labels, and shortcuts; keep selected-state contrast in light mode. | src/styles/global.css:161, src/styles/global.css:174 |
| I05 | P1 / S | Replace mouse-hover movement and glow on noninteractive cards with quiet borders. | src/styles/global.css:459 |
| I06 | P1 / S | Stronger title hierarchy, tighter radii, and less visual weight on status badges. | src/styles/global.css:425 |
| I07 | P1 / S | High-contrast neutral primary buttons; accent remains an orientation/focus signal, not a tint on every surface. | src/styles/global.css:525 |
| I08 | P1 / S | Increase light-mode muted/status text contrast and set native control color-scheme from the effective theme. | src/lib/theme.tsx:47, src/styles/global.css:158 |
| I09 | P2 / S | Move appearance controls into an expandable sidebar section without removing any theme, accent, or density option. | src/components/AppShell.tsx:205 |
| I10 | P1 / M | Reach console and inspector from every view, with pressed states on their toolbar buttons. Console starts closed instead of duplicating Runner output. | src/components/AppShell.tsx:225, src/App.tsx |
| I11 | P1 / S | Toggle console without unexpectedly navigating away from the user's current work. | src/App.tsx:85 |
| I12 | P1 / S | Reserve space for mobile navigation; hide the closed drawer from keyboard focus. | src/styles/global.css:1604 |
| I13 | P1 / S | Add skip-to-content and focus-visible styling; remove the whole-main live region so every page change is not announced as a log update. | src/components/AppShell.tsx:142, src/components/AppShell.tsx:229 |
| I14 | P0 / S | Scope sidebar arrow navigation to navigation; keep Enter/Space on native buttons and ignore editing/dialog targets. Escape dismisses the drawer. | src/components/AppShell.tsx:95 |
| I15 | P1 / M | Make the project brief the primary surface; keep workspace setup beside it, not above it in a long form. | src/views/ProjectView.tsx:157 |
| I16 | P1 / S | Autosave the draft locally so navigating to inspect something does not erase the brief. | src/views/ProjectView.tsx:21, src/views/ProjectView.tsx:82 |
| I17 | P0 / S | Plain Enter makes a newline; Ctrl/Cmd+Enter explicitly submits; avoid submission during IME composition. | src/views/ProjectView.tsx:117 |
| I18 | P0 / S | Guard both keyboard and button submissions during execution, workspace loading, or creation. Root run entry also rejects an already-active run. | src/views/ProjectView.tsx:105, src/App.tsx |
| I19 | P1 / S | Three useful starter briefs: web application, API service, internal tool. Clearly label these as editable prompts, not prebuilt templates. | src/views/ProjectView.tsx:22 |
| I20 | P0 / S | Recent goals and starter cards load into the composer for review instead of triggering surprise execution. | src/views/ProjectView.tsx:140 |
| I21 | P0 / S | Validate the persisted recent-goal data shape so a malformed JSON object cannot crash the page. | src/views/ProjectView.tsx:42 |
| I22 | P0 / M | Put simulation versus file-writing behavior beside the run action. Do not let a simulation look like a generated project. | src/views/ProjectView.tsx:174 |
| I23 | P1 / S | Use backend-resolved workspace IDs and roots; show the active managed root and workspace count. Preserve legacy selection migration. | src/views/ProjectView.tsx:49 |
| I24 | P1 / S | Workspace creation/loading feedback, disabled duplicate controls, and an explicit recovery action for fetch failures. | src/views/ProjectView.tsx:85, src/views/ProjectView.tsx:208 |
| I25 | P0 / S | Expose execution context and generation requirements without implying saved legacy provider records configure the generation gateway. | src/views/ProjectView.tsx:213 |
| I26 | P1 / S | Direct links from the project to saved goals, specialized-agent tasks, history, and gateway inspection. Every link uses the existing view switcher. | src/views/ProjectView.tsx:224 |
| I27 | P1 / M | Adapt the project columns to available content width and stack them for narrow windows/open inspectors. | src/styles/workbench.css:68 |
| I28 | P0 / S | Show plan progress only when steps exist; otherwise say measurements are unavailable. Never derive 100% from a generic completed flag. | src/views/RunnerView.tsx:142 |
| I29 | P0 / S | Distinguish running, stopped, and finished progress copy; a failed run should not say “Waiting for a run.” | src/views/RunnerView.tsx:145 |
| I30 | P1 / S | Calculate elapsed time from the run-start timestamp so leaving/reopening Runner does not reset it. Omit unavailable durations. | src/views/RunnerView.tsx:98, src/App.tsx |
| I31 | P1 / S | Use consistent SVG plan-state icons and theme-aware repair text. | src/views/RunnerView.tsx:20, src/views/RunnerView.tsx:256 |
| I32 | P1 / S | Search console text together with severity/source filters; provide explicit accessible filter labels. | src/views/RunnerView.tsx:57, src/views/RunnerView.tsx:386 |
| I33 | P0 / S | State the newest-300-event display limit and filtered count instead of implying all events are visible/exported. | src/views/RunnerView.tsx:445 |
| I34 | P1 / S | Report clipboard success/failure; disable empty copy/export and unwired clear actions; make scroll-to-bottom a keyboard button. | src/views/RunnerView.tsx:67, src/views/RunnerView.tsx:484 |
| I35 | P0 / S | Distinguish degraded verification from an unqualified pass in both Runner and run-history details. | src/views/RunnerView.tsx:371, src/components/RunCompletionSummary.tsx:48 |
| I36 | P0 / M | Show history-detail loading/errors before a result exists; allow retry and ignore late responses after another selection or Close. | src/views/RunHistoryView.tsx:83, src/views/RunHistoryView.tsx:155 |
| I37 | P0 / S | Stabilize the history hook callback to avoid effect-driven fetch loops; expose list failures separately from an empty history and offer refresh/retry. | src/hooks/usePipelineRunHistory.ts:7, src/views/RunHistoryView.tsx:59 |
| I38 | P0 / S | Correct Plans/History copy that implied nonexistent execute-from-plans or comparison tools; consistently name AI gateway. | src/views/PlansView.tsx:33, src/views/RunHistoryView.tsx:109, src/views/AIView.tsx:28 |

## Broader capability-specific roadmap

These are deliberate follow-ups, NOT dummy controls added to the UI. Some can be implemented in presentation code; others require new runtime contracts. None is claimed delivered by this change.

| ID | Priority / effort | Idea | Evidence / prerequisite |
| --- | --- | --- | --- |
| R01 | P0 / M | Server-derived readiness checklist: native runtime, model endpoint, model, authentication, workspace writeability. | ProjectView currently exposes requirements, not a live health verdict; add a typed health/preflight command before showing green checks. |
| R02 | P0 / L | Connect provider settings to the generation runtime with masked credentials, endpoint validation and a real test-connection result. | SettingsView describes provider entries as legacy metadata; ProjectView records the environment-only generation requirement. |
| R03 | P0 / L | Persist run ownership and resume/attach after restart; preserve cancellation as a distinct state instead of a generic failure. | RunState in src/types/frontend.ts and current App run lifecycle are session-oriented. |
| R04 | P0 / L | Explicit write preview, overwrite approval, checkpoint, and rollback before altering existing project files. | Project generation writes managed files; add transactional backend support before offering Undo. |
| R05 | P1 / L | Real generated-file explorer and unified diff with file status, errors, safe read access, and open-in-editor actions. | Runner currently presents paths/statuses; UI needs safe content/baseline access and native editor-launch contracts. |
| R06 | P1 / L | Live task/agent graph with dependencies, queued/running states and per-stage timings. | Runner only knows the steps supplied by RunState; add structured live plan updates, not progress guessed from log strings. |
| R07 | P1 / L | Build preview in an isolated process/window with readiness, port conflicts, process shutdown and log linkage. | No preview lifecycle UI currently exists. Do not embed an arbitrary provider URL as a trusted application preview. |
| R08 | P1 / M | Saved-plan search, edit, deletion confirmation, and “Use as brief” before execution. | src/views/PlansView.tsx has a saved-goal list, not a full editable execution plan. |
| R09 | P1 / M | Structured task-result/Markdown rendering with selectable code blocks and copy feedback in Agent chat. | src/views/AgentChatView.tsx; choose a sanitizer/renderer after auditing dependencies. |
| R10 | P1 / L | Agent token streaming, visible tool requests, approvals and Stop; distinguish agent task cancellation from pipeline cancellation. | AgentChatView task execution is not a token-streaming session contract. |
| R11 | P1 / M | History pagination, status filters, run comparison and workspace-bound rerun review. | src/views/RunHistoryView.tsx currently requests 20 records and reruns a goal using the active app context. Persist/select the original workspace before claiming reproducibility. |
| R12 | P1 / M | Read-only error details with file/line links, correlated run IDs, and explicit remediation actions in Diagnostics. | src/views/DiagnosticsView.tsx and runtime monitor-event contract. |
| R13 | P1 / M | Show telemetry distributions/trends and sample counts, not decorative live charts; expose missing measurements. | src/views/TelemetryView.tsx and benchmark hooks provide the data boundary to extend. |
| R14 | P1 / M | Link learned patterns back to originating runs, show confidence/sample evidence, allow pin/disable with persistence. | src/views/LearningView.tsx; extend persisted pattern provenance/actions where absent. |
| R15 | P1 / M | Resizable inspector/console with persisted layout, min sizes and a one-action reset. | AppShell uses fixed CSS dimensions; requires keyboard-accessible resize handles and drag testing. |
| R16 | P1 / M | Consistent field IDs, form errors and async recovery across every secondary screen; audit remaining clipboard actions. | Shared Field permits omitted htmlFor; not every legacy form was rewritten here. |
| R17 | P1 / M | Complete focus trapping/inert background for mobile navigation and palette; route-change focus restoration and full screen-reader pass. | AppShell has scoped keys/Escape and hidden closed nav, but this is not a completed accessibility audit. |
| R18 | P2 / M | Optional native notifications and run completion sounds, disabled by default; include meaningful completion/failure text. | App toasts exist; native notifications require permissions and desktop testing. |
| R19 | P2 / M | First-run onboarding that configures a real endpoint, creates a managed workspace and verifies one small generation end to end. | Depends on R01/R02; do not add an onboarding tour that checks off unverified readiness. |
| R20 | P1 / M | Draft retention controls, per-workspace drafts, local-data export/clear and log redaction. | The new brief and existing history persist locally; add data lifecycle controls and classify sensitive logs before broad export. |

## Do not regress

- The backend remains authoritative for workspace IDs and roots; never trust a saved arbitrary filesystem path.
- Plan simulation must not be advertised as generated files or a verified build.
- Unknown step progress, missing measurements, and degraded checks are not successes.
- No disconnected Cancel/Preview/Import/Rollback buttons.
- No provider secrets in screenshots, logs or exported diagnostics.
- Preserve all routes, IPC normalization, existing failure details, theme choices, keyboard command palette and console timestamps.
- Do not execute starter/recent briefs merely because a user selects them.
- Keep destructive maintenance actions explicit and preserve their existing confirmations.

## Verification

- PASS: `npm run build` (TypeScript project build + Vite production build).
- Output: `dist/index.html`; CSS 40.51 kB (7.63 kB gzip); JavaScript 350.27 kB (101.68 kB gzip). No baseline bundle delta is available.
- Regression sources: ProjectView.test.tsx (9 cases), AppShell.test.tsx (5), RunnerView.operations.test.tsx (4), RunHistoryView.test.tsx (4). These include new coverage for safe briefs, busy guards, unknown progress, filtering and history recovery.
- NOT RUN: behavioral tests. The requested test execution was denied; it was not retried by another command.
- NOT VERIFIED: rendered desktop/narrow/light/dark layouts and visual contrast. Starting the preview server was denied; no substitute server or fabricated screenshot was used.
- NOT VERIFIED: native Tauri/provider/file-generation end-to-end behavior. The work is presentation-focused; a production frontend build is not a runtime end-to-end test.
- Git comparison unavailable: this directory does not contain discoverable Git metadata. No commits or pushes were made.

Acceptance still required: approve running the tests, then inspect Project/Runner/History at desktop and narrow widths, open both global panels, switch themes and accents, navigate exclusively by keyboard, and execute one real managed-workspace run with its verification evidence visible.
