# recode Roadmap

**Version:** 2026-10-15
**Author:** Autonomous Hermes Agent

## Implementation Summary

| Backlog | Open items | Items fully closed (`[x]`) |
|---------|-----------|---------------------------|
| P0 — secure and honest substrate | 5 | 0 |
| P1 — first real Vite/React generation | 9 | 0 |
| P2 — agent, planning and provider parity | 5 | 0 |
| P3 — operate, extend and replace legacy | 4 | 0 |
| **Total** | **23** | **0** |

Every backlog item is currently `[ ]` (open) with inline **Implemented slice** notes describing the partial, test-verified progress to date. No item has been fully closed: each still lists concrete remaining work (acceptance, live-provider or native-IPC evidence, or unported devcode behavior). An item moves to `[x]` only when its **Open** list is empty and the acceptance matrix for that slice is satisfied. Do not infer completion from passing unit tests alone — porting principle 5 (no false green).

---

> Porting target: replace CodeForge's real end-to-end workflows in a Tauri 2 desktop app, not merely reproduce its screens or module names.
> Status is based on executable wiring and demonstrated behavior; a struct, mock, view, or unused module is not feature parity.
> This is the canonical delivery plan. `docs/recode-roadmap.md` is historical architecture context, not an independent status list.

## Current State (source-reconciled; historical test checkpoints retained below)

Latest execution policy: the user approved proceeding after the explicit request to run focused native policy, local HTTP integration and frontend evidence tests. Those checks and a focused SQLite history regression have now run (see checkpoint below). This does not authorize preview startup, live provider calls or imply full-suite/visual acceptance. `UI-WORKBENCH-REDESIGN.md` is a detailed design inventory; this file owns delivery status and dependency order.

Status vocabulary: **working** = reachable and tested within stated scope; **partial** = some wiring exists but cannot perform the source workflow; **missing** = no credible end-to-end path; **unverified** = present but not exercised against a real project/provider. Do not equate passing unit tests with replacement readiness.

### Evidence-backed capability inventory

| Capability | Status | Evidence and remaining gap |
| --- | --- | --- |
| Desktop shell, view navigation, theme, palette | Implemented; latest redesign build-verified, interaction/visual acceptance open | `src/App.tsx`, `src/components/AppShell.tsx`, `src/styles/{global,workbench,runner}.css`; prior UI test checkpoints do not verify the redesigned surfaces. |
| IPC wrappers and tests | Partial | `src/lib/ipc.ts`, `ipc.test.ts`, commands registered in `src-tauri/src/main.rs`; types are hand-mirrored, not generated Rust contracts, and coverage is not proven for every command. |
| SQLite, plans, agent history, team tasks, quality, learning patterns | Partial | `src-tauri/src/{app_state,persistence,learning}.rs`; persistence is fragmented; real execution/history round trip needs testing. |
| Workspaces and file CRUD | Partial; **security prerequisite** | Tauri IPC uses registered IDs under `app_data_dir/projects` instead of webview-supplied roots; `workspace/mod.rs` rejects unsafe IDs/paths, common private filenames and pre-existing symlinks, stages synced same-directory replacements, and bounds file reads/listing. Still vulnerable to path check/use races, lacks legacy external-folder import and reconciliation/install/start loop. |
| Plan pipeline | Partial; simulation plus registered full-manifest generation | `tauri_commands.rs:295` → `LocalPipelineService::run_internal` → `pipeline/mod.rs:323` calls `LocalPipelineEngine::generate_project_files_cancellable`. Generation now emits exactly the missing Vite required manifest (9 files) dynamically instead of a hardcoded 2 files, handles completion tracking, and verifies outputs on disk. |
| Agent execution and routing | Partial (simulation) | `agent_runtime.rs` returns a synthetic completion; `agents.rs` has a registry; descriptors are not a dependable smart-routing/execution service. |
| Gateway/provider settings | Partial (mock-HTTP verified) | `gateway/openai.rs` sends non-streaming OpenAI-compatible requests from environment-only configuration. Local HTTP tests prove POST/response and 429 classification, not a live provider. SQLite refuses new plaintext keys and returns no legacy keys to the webview, but previously stored secrets still require migration; cancellation, streaming, retry policy, model capabilities and accounting remain open. |
| Diagnostics, healing, dev server, updater, quality | Partial/unverified | Corresponding Rust modules, commands and UI exist; test against actual generated workspace/runtime, not only synthetic events. |
| Real generation, reconciliation, verification, repair, truthful outcome | Partial/unverified | Registered workspaces can reach provider-backed file writes and a content-on-disk persistence gate; mocked provider → disk → verify is tested. Live provider/desktop invocation, compile/boot/test/lint gates, repair, and terminal outcome semantics remain unverified or missing. |
| Advanced agent learning, approvals, multi-runtime, GitHub, deployment | Missing/partial | Separate persistence and UI fragments do not establish feature parity. |
| Saved-goal library and review intents | Implemented; acceptance open | `usePlans.ts` surfaces load/save errors, retains failed-save drafts, uses UUID/upsert retry IDs and stale-response protection. `PlansView.tsx` adds search/refresh/review; App passes typed review requests into Project instead of executing a history browser event. This is not a versioned approved plan or replayable historical workspace. |

### Saved-plan / readiness checkpoint — implementation, not behavioral sign-off

- Q1/Q2: saved-plan library and review-intent flow implemented across `usePlans`, Plans, Project, Runner, History and App. Goal selection does not execute work; existing draft replacement is explicit. Same-goal save retry reuses the native upsert ID within the current mounted session; durable idempotency across app restart remains open.
- Q3: Agent chat explicitly discloses simulated acknowledgments. Nine additional role contracts are designed below; none is misrepresented as an implemented agent.
- Q4 initial slice: `GatewayConfigurationCheck.tsx` uses only `get_gateway_status` on explicit click, with malformed-response rejection in `ipc.ts`. This is not an active provider connection test or complete native preflight report.
- **PASS:** latest direct `npm run build` executes `npm run type-check` (`tsc --noEmit`) and Vite production bundling, exit 0. Output: CSS 41.91 kB / 7.85 kB gzip; JS 356.27 kB / 103.61 kB gzip. JS is 6.00 kB larger than the prior verified UI bundle (350.27 kB); without Git isolation this is a working-tree comparison, not exclusive attribution.
- **Authored, NOT RUN:** 13 additional regression cases: `src/hooks/usePlans.test.ts` (4), `src/views/PlansView.test.tsx` (3), `src/components/GatewayConfigurationCheck.test.tsx` (3), and 3 added App integration cases. Tests/linters and preview startup were not retried after denial. An automatic file-writer check on the hook test used incompatible standalone TS defaults; the canonical project production build passed, but no test execution is inferred.
- **Still unverified:** rendered layouts, keyboard interaction in a browser, native Tauri invocation, SQLite/provider end-to-end flows and runtime permission enforcement. Rust was inspected, not modified or re-tested in this continuation.
- **Next blocking capabilities:** native scoped grants and structured readiness; durable run/project/plan identity; approved complete manifests; transactional write recovery; applicable compile/boot/test evidence. These remain open despite frontend build success.

Validation after the pipeline-control/IPC slice (2026-09-26): the two focused Rust pause/abort tests passed via a disposable `/tmp/hermes-verify-*` script (removed afterward), `cargo check -p recode --bin recode` passed with existing warnings, targeted frontend `vitest` **23/23 passed**, and `npm run build` **passed**. The earlier no-retry denial was lifted by explicit user approval. Before this slice, `npm run test` **53/53 passed** and `cargo test --workspace` **181 Rust unit tests plus integration tests passed**; the full suites have **not** been rerun after the slice. An initial parallel Rust suite had failed in `app_state` because tests deleted a shared SQLite temp DB; isolated test DB names fixed the race. No real generation parity or native desktop invocation has been demonstrated. `recode` has no `.git` directory; preserve unrelated files and do not assume commits are possible.

Validation after the registered-workspace, gateway and bounded-file-IO slices (2026-09-26): `cargo test --workspace -q` **191 unit tests plus integration tests passed**, `cargo check -p recode --bin recode` **passed with existing warnings**, `npm run test` **55/55 passed** (existing `act(...)` warnings in `useGatewayRun.test.ts`); the newly added ProjectView test was rechecked **3/3** without warning after an `act` fix, and `npm run build` **passed**. Mock loopback HTTP tested POST dispatch, selected model ownership and redacted rate-limit errors. A live model call, native desktop IPC invocation, generated-file write, and release bundle remain unverified. The historical validation note above reflects its earlier checkpoint; these later full suites supersede it.

Validation after terminal-history and dev-launcher slices (2026-09-26): `cargo test --workspace -q` **196 unit tests plus integration tests passed**, `cargo check -q -p recode --bin recode` **passed with existing warnings**, `npm run test` **55/55 passed** (existing hook `act(...)` warnings), `npm run build` **passed**, and `bash -n scripts/tauri-dev-with-vite.sh` plus an occupied-port smoke test confirmed the script refuses to kill an unrelated listener. Browser navigation confirmed Project, AI and Providers text and no JavaScript exceptions; screenshot analysis service returned HTTP 404, so visual layout and native IPC are not claimed. A native `cargo run` attempt required a large debug rebuild; the Vite process exited -9 during it, and compilation was stopped when the filesystem had only ~1.4 GiB free. Native invocation and bundle remain unverified; do not remove pre-existing build artifacts or unrelated processes to make room.

### Source-of-truth mapping and scope

- Source project: `/home/z/Projects/devcode`; authoritative behavior in `server/src/services/{ServerWorkspaceManager,WorkspaceReconciliationPipeline,ServerBuildVerifier,ServerDevServerManager}.ts`, `server/src/services/runtimes/`, `src/application/services/{pipeline,build-pipeline,agents,planning,ai-gateway}/`, and `server/src/agents/`. Inspect nearest source `AGENTS.md` and its tests before each porting slice. Paths may move; verify before editing.
- Preserve the *user-visible workflow* and failure semantics, not legacy server transport, Fastify/React internals, arbitrary agent counts, or accidental defaults. Record a capability with a source path, destination path, contract test, and live acceptance proof when starting each slice.
- Explicit decision: ship a secure, local, single-user, Node/Vite/React vertical slice before additional runtimes, remote hosting or broad multi-agent catalog. Never execute arbitrary generated install scripts without an explicit consent/trust boundary; no hidden network/package install in a read-only action.

## Porting Principles

1. **Incremental: one working slice at a time.** Each item should leave the app runnable.
2. **Typed IPC boundary first.** Every Rust capability gets a typed Tauri command + TS wrapper before any view consumes it.
3. **Contract-first, one owner.** Rust owns serialized request/response schemas; generate TS types or maintain explicit, tested mirrors until generation exists. Domain errors map once at IPC.
4. **Desktop-first accessibility.** Optimize the development workbench for keyboard use, screen readers, small windows and adjustable density/type size. Validate large-screen use separately; do not impose a TV-specific layout or microscopic dashboard density on every user.
5. **No false green.** Persist per-file and per-gate failure states; unknown, skipped and cancelled are not success. Never call an unused implementation complete.
6. **Port fixes, not just features.** Truncation handling, bounded retry, write barriers, conflict visibility and truthful outcomes ship with the components that need them.
7. **Bound authority.** The webview is not a trusted filesystem authority: validate canonical workspace ownership in Rust, require explicit consent for execution/network/deployment, redact secrets in logs, and limit generated process privileges.
8. **No silent source parity assumptions.** Decide each feature by user workflow and tests, retain an explicit deferral register, and update statuses only after executable acceptance checks.

### Latest verification-evidence checkpoint

- Shared evidence panel is wired through Runner and `RunCompletionSummary` (including History and failed-run detail). It distinguishes reported pass, failure, limited check, unknown status and no reported results. Missing results now have an explicit empty state rather than disappearing.
- Source review found that the earlier `degraded ? warning : passed ? success : failure` ordering downgraded explicit failed checks to warnings. The common classifier now gives known failure precedence; unknown markers prevent absent booleans from becoming fake failures.
- Legacy records do not supply applicability, commands, exit codes or tool versions. The panel states this limitation and never upgrades a list of reported passes to an overall verified build. It retains original runtime summaries as secondary detail and does not invent gate names for the legacy `default` check.
- **PASS:** latest direct `npm run build`, exit 0, including project type-check and Vite bundle. CSS 43.42 kB / 8.06 kB gzip; JS 358.64 kB / 104.49 kB gzip. Working-tree changes from the preceding checkpoint: +1.51 kB CSS, +2.37 kB JS; no isolated Git baseline.
- **Authored, NOT RUN:** 9 classifier/normalization/IPC regression cases in `src/lib/verification.test.ts`, 4 shared-panel cases in `src/components/VerificationEvidence.test.tsx`; updated the Runner limited-evidence assertion. Prior test/preview denials remain in force. Automatic standalone test-file type checks again reported dependency target/module-resolution errors; canonical project build passed, not a test run.
- **Still open:** native gate policy, migration and run identity, live IPC/provider/SQLite acceptance, rendered verification and the full test suite. No Rust services were modified by this evidence UI slice.

### Native required-gate checkpoint — implemented / compile-checked, acceptance open

- Source mapping: `devcode/server/src/services/ServerBuildVerifier.ts` distinguishes commands that actually ran and their exit codes. Destination: `src-tauri/src/pipeline/{verify,outcome,engine}.rs`, `src/types/frontend.ts`, `src/lib/verification.ts` and the shared `VerificationEvidence` panel. This ports evidence semantics and outcome enforcement, **not** the subprocess verifier itself.
- The live registered-workspace path now evaluates a native-owned required set: persistence, typecheck, bundle, tests and startup. Missing/duplicate records, unsupported versions, unresolved applicability, skipped/unknown checks, degraded passes and absent process receipts cannot satisfy the policy. Result-provided `required=false` / `applicable=false` cannot waive it. Explicit failures and nonzero exit codes take precedence over missing evidence.
- The existing byte comparison is named `persistence` and reports an in-process result. The other four checks are explicitly unknown with null execution receipts: no command, duration or tool version is fabricated, and no generated process is launched. **Behavior change:** even a successful two-file generation now returns `outcome.success=false` with “verification blocked”; existing native history classification stores it as failed with inspectable files/evidence. Unknown checks are not displayed as measured failures. Simulation does not acquire these build gates.
- `VerificationResult.evidence` is an optional serde-defaulted v1 envelope: status, requirement, nullable applicability and optional argv/exit/duration/tool-version receipt. Old JSON remains legacy evidence; it is not promoted to a fresh pass. No SQLite table migration was needed for the nested JSON field; reopen/round-trip acceptance is still outstanding. No historical rows were rewritten.
- Frontend normalization retains the envelope, distinguishes skipped/unknown from failure, rejects malformed receipts/unsupported versions, and preserves explicit failures even when other metadata is malformed. Runner and History share requirement/applicability/receipt presentation and warn on success plus unresolved required checks. This display does not replace native enforcement.
- **PASS:** direct `cargo check -p recode --lib --bin recode`, exit 0 (35 warnings), and latest direct `npm run build`, exit 0 (project TypeScript check + Vite). Bundle: CSS 43.42 kB / 8.06 kB gzip, JS 361.57 kB / 105.31 kB gzip. Rust check is compilation, not execution or a packaged desktop build.
- **Authored, NOT RUN:** 7 native policy/serde cases in `pipeline/outcome.rs`, 11 normalization/IPC/presentation cases in `src/lib/verification.versioned.test.tsx`; updated the existing loopback provider test to expect persisted files but blocked verification. Prior test/preview denials remain in force. Automatic file-write checks reported Rust formatting differences; formatting/lint acceptance is not claimed.
- **Still required before autonomy:** native scoped grants, approved complete manifests, race-safe/journaled writes, supervised allowlisted command execution, startup readiness assertions, tool provenance and workspace/plan/attempt hashes binding each receipt to the artifact it checked. The receipt schema validates shape, not command authenticity or coverage: only trusted native verifiers may emit it, never agents/webview submissions. No such submission command was added. Native IPC, SQLite reopen, live provider and rendered UX remain unverified.
- **Next delivery order:** exercise the authored negative cases when testing is authorized; then scoped grants + immutable plan/workspace identity, complete staged manifests, and consented supervised verifiers. Do not add agent breadth, arbitrary shell access, automatic installs or parallel writers to bypass these prerequisites. Q5 remains partial.

### Native immutable identity checkpoint — implemented / compile-checked, acceptance open

- Source mapping: `devcode` durable operations require plan/workspace/run identity rather than ad-hoc run IDs. Destination: `src-tauri/src/pipeline/identity.rs` and `src-tauri/src/pipeline/identity_persistence.rs`, plus `mod identity[_persistence];` registration in `pipeline/mod.rs`.
- Added immutable record types: `PlanRevision`, `RevisionStatus`, `WorkspaceGrant`, `RunIdentity`. Added SQLite repositories for plan revisions, workspace grants and run identities, with schema creation, insert/list/get paths and reopen tests.
- Verified: `cargo test -p recode --lib pipeline::identity::tests` passed **3/3**; `cargo test -p recode --lib pipeline::identity_persistence::tests` passed **3/3**; `cargo test -p recode --lib pipeline_history::tests` passed **7/7**; `cargo check -p recode --bin recode` passed. These are native compilation/focused-test checks only.
- Frontend/IPC wiring, provenance display regression, and `npm run build` remain unverified because that command was denied and not retried. These types are not yet exercised by any Tauri command or view; do not claim durability acceptance from type-level tests alone.
- Next slice after frontend approval: bind `RunIdentity`/`WorkspaceGrant` into `run_pipeline` and history detail, preserving the existing fail-closed gate policy. Do not enable replay, auto-restore, or agent submission of receipts/identities.

### Focused verification and SQLite compatibility checkpoint

- After approval to proceed, direct `cargo test -p recode --lib pipeline::` passed **35/35**, exit 0. This includes all 7 native policy/serde cases, negative generation/cancellation cases and the loopback HTTP provider → persisted bytes → blocked-build outcome integration. It is not a live model or desktop invocation.
- Direct `npx vitest run src/lib/verification.test.ts src/lib/verification.versioned.test.tsx src/components/VerificationEvidence.test.tsx src/lib/ipc.test.ts` passed **103/103** across 4 files, exit 0. The 11 versioned cases and previous legacy/panel cases are no longer unrun. These are mocked IPC/jsdom checks, not native desktop or visual acceptance.
- Added `pipeline_history.rs::tests::blocked_versioned_evidence_and_legacy_results_survive_reopen`: isolated on-disk SQLite, typed PipelineRun serialization/deserialization, legacy and v1 unknown evidence in both nested and top-level fields, terminal failure preservation and duplicate finalization rejection after reopening. Direct `cargo test -p recode --lib pipeline_history::tests` passed **7/7**, exit 0, including the new case. Test fixtures are synthetic and do not establish execution of generated commands.
- This supersedes the unrun-test and schema-reopen limitations in the preceding checkpoint for these exact cases only. No production logic changed in this verification continuation; full suites, native IPC, process-restart recovery, live provider and rendered UI remain unverified. Rust compilation emitted 25 warnings; no unrelated cleanup or broad formatting was attempted.
- Next structural work remains scoped native grants and immutable plan/workspace/attempt identity, followed by approved complete manifests and supervised verification. Reopen compatibility does not grant replay authority or bind old receipts to current workspace bytes. Q5 remains partial, with focused policy/serialization/persistence evidence now established.

## Critical design review — decisions that change the execution order

1. **Do not rebuild a second generator.** Source inspection confirms the engine is reachable; only the similarly named manifest helper remains simulated. Extend the live `pipeline/types.rs` service, then remove redundancy with parity evidence. Otherwise months of work can land on an unused path.
2. **Approval is a substrate, not a late collaboration feature.** Basic scoped consent must precede generated-file overwrite, dependency scripts, child processes, provider egress and deployment. Team proposal UX may wait until Phase 6; the native enforcement boundary may not.
3. **Provider readiness precedes agent breadth.** One dependable model path with timeouts, redaction and truthful health is more valuable than many registered names. The three current local agents acknowledge tasks synthetically; the UI now discloses that limitation.
4. **Separate intent, approval, attempt and evidence.** A saved goal is not a plan revision; accepting a brief is not approving a tool call; a generated file is not a verified app; a completed plan is not a successful build.
5. **Keep the existing modular desktop architecture.** A Rust modular monolith plus typed React adapters is sufficient. No microservices, extra event brokers, plugin marketplace or blanket state-library migration without a concrete isolation/ownership need.
6. **Structured operations, not arbitrary agent shell access.** Agents request typed tools with path scopes and bounded argv. A deterministic policy layer decides eligibility; native services execute. An LLM cannot approve itself or change its own budget.
7. **Transactional writes before parallel writers.** Temporary files make single replacements atomic, not a whole run reversible. File ownership, baseline hashes, conflict handling and a write journal come before multi-agent parallel editing.
8. **Independent evaluation, not majority votes.** Verifiers produce command/exit/artifact evidence. Review agents may identify risks, but a vote cannot override a failing required gate. Measure routing and repair quality on fixed fixtures before adding adaptive policies.
9. **Treat all replay as a new request.** Until runs persist an immutable workspace/plan/model revision, history reuse loads a reviewable brief. Never imply identical reproduction or silently apply an old goal to the current workspace.
10. **Make comprehensive planning usable.** The workstream catalog below is the full destination; the delivery queue is deliberately short. Speculative features stay designed/open, not cosmetically enabled. Every closure needs negative-case evidence and a real user path.

## Target architecture — incremental evolution, not replacement

### Ownership and trust boundaries

```text
React views / local draft state
  -> typed IPC adapters (validation, normalized domain errors)
  -> native application services (identity, policy, durable state)
       -> run coordinator / plan revision store / event journal
       -> workspace service / staged artifacts / write journal
       -> provider gateway / keyring references / usage ledger
       -> process supervisor / tool broker / deterministic verifiers
  <- snapshots + sequenced events + artifact/evidence references

Generated preview: separate unprivileged origin/process, no Tauri bridge.
Agents: bounded clients of the tool broker, never filesystem authorities.
```

Existing modules are starting points, not migration casualties: `App.tsx` composes navigation; `src/hooks/` owns view orchestration; `src/types/frontend.ts` owns shared frontend contracts; `src/lib/ipc.ts` is the transport adapter. Rust retains `app_state`, `persistence`, `workspace`, `pipeline`, `gateway`, `agent_runtime`, `agents` and the native command registration. New modules below are **proposed**, not claims they already exist.

| Owner | Proposed responsibility and seam | Invariant / failure behavior |
| --- | --- | --- |
| Native policy service | `policy.rs`: grants, operation scopes, expiry, approval receipts | Default deny for missing/mismatched grants; no frontend-only permission enforcement. |
| Run coordinator | Evolve `pipeline/mod.rs` and history into one coordinator, not another scheduler | At most one terminal transition per attempt; preserve evidence on failure; cancellation request is distinct from stopped work. |
| Event journal | Extend persistence/event bus with bounded durable sequence records | Per-run monotonically increasing sequence; replay from cursor; UI gap detection triggers snapshot recovery. |
| Artifact/write service | Evolve `workspace/` with staged manifests, hash preconditions, journal, commit/recovery | Never replace a user-edited file against an old baseline; partial commit is inspectable and recoverable. |
| Tool broker | `tools/`: typed operations for approved workspace reads/writes and subprocesses | No shell interpolation; deny unknown tool/schema; permission and budget checked before and during work. |
| Gateway | Extend existing gateway with shared provider config/keyring/limits | Secrets never round-trip to webview; explicit opt-in remote data transfer; no silent privacy-changing failover. |
| Process supervisor | Evolve dev-server/installer ownership into one supervised-child abstraction | Track child/process-group ownership; bounded output/deadline; never kill an unrelated listener or PID. |
| Outcome evaluator | Pure `pipeline/outcome.rs` over applicability and evidence | Required failed/unknown/skipped gate cannot become verified; warnings and partial generation remain visible. |
| Agent scheduler | Extend `agents.rs` / `agent_runtime.rs` via an injected executor | Do not hold registry mutex during model/tool work; reserve task budget and file leases before dispatch. |
| UI review intents | `BriefReviewRequest` and explicit callbacks, implemented now | Navigation intent cannot execute work; retain current draft until user accepts replacement. Not a native security grant. |

### Contracts to define before enabling new runtime behavior

- **WorkspaceRef**: registered ID, revision, authorized root identity. Never serialize an arbitrary path as authorization. Operations resolve it natively.
- **PlanRevision**: stable plan ID + revision, source brief, structured tasks/dependencies, acceptance criteria, manifest, baseline hashes and status `draft / reviewed / approved / superseded`. Preserve old saved-goal records as unapproved drafts.
- **RunIntent**: intent ID, exact plan/workspace revisions, requested mode, provider/model selection, budget, required gates, approval reference and idempotency key. Revalidate at dispatch; changed manifest invalidates approval.
- **ApprovalReceipt**: operation digest, project/plan scope, allowed paths/tools/network destinations, issuer, issue/expiry time, one-shot/reusable policy. Never accept a boolean `approved: true` from arbitrary webview input as proof.
- **TaskAttempt**: run/task/attempt IDs, role/version, input artifact hashes, owned paths, dependencies, deadline, token/tool/output caps, state and cancellation reason. Record retries as attempts, not new unrelated successes.
- **ToolRequest / ToolResult**: typed tool name/version, validated arguments, scope and policy decision; command/argv/working-directory references, start/end, exit status, truncated-output flag and evidence artifact. Redact before persistence.
- **RunEvent**: schema version, run/attempt/project identity, sequence, observed timestamp, topic, severity and typed payload. No UI completion inferred from free-form log strings.
- **ArtifactRef**: ID, kind, logical relative path, byte length, content hash, originating attempt and retention class. Large content lives outside SQLite rows; history keeps bounded metadata and references.
- **GateEvidence**: gate ID/version, required/applicable flags, `passed / failed / skipped / unknown`, reason, command, exit code, duration and artifact refs. Provider-generated test text is not a passing test execution.
- **ReadinessReport**: checks with `ready / blocked / unknown`, read-only check scope, reason/remediation, observed time and configuration revision. A network probe or write probe must be explicitly distinguished from passive inspection.

Contract migration policy: version wire envelopes, add backward-compatible serde defaults where appropriate, normalize once at IPC, retain old records with explicit missing fields, and test round trips in both Rust and TS. Adding optional Rust fields still requires updating every Rust struct literal; JSON defaults do not make source constructors optional.

### Run lifecycle and resource policy

Proposed lifecycle: `draft -> awaiting_approval -> queued -> running -> verifying -> succeeded | failed | cancelled | interrupted`; `running -> cancelling -> cancelled` may include bounded cleanup. `verified` is a derived evidence classification, not an optimistic UI event. Pause applies only at documented cooperative safe points; display `pausing` until acknowledged.

- Give each workspace an operation lease; use fencing/revision tokens so a recovered stale owner cannot write after its lease is replaced. Do not mark another live process's work interrupted during startup.
- Initial scheduling is sequential. Enable bounded independent DAG tasks only when they own disjoint files and dependencies have produced immutable artifacts.
- Budgets must cap per-call, per-attempt and whole-run tokens, tool calls, elapsed time, generated/staged bytes, event volume and child output. Account for retry/repair overhead before dispatch, not after a surprise bill.
- Recovery must surface uncertain outcomes; never replay a paid model call, install script or deployment automatically because an acknowledgment was lost. Check an idempotency record or ask for a new action.
- Data retention separates ephemeral drafts, durable evidence and secrets. Provide export/delete controls; SQLite backup/migration and artifact garbage collection must not race active runs.

## Agent product design — proposed roles, not a fake expanded registry

Ship each role only with an executable handler, constrained tools, failure tests and a measurable output contract. Role configuration selects a model/prompt policy; it does not create an independent unrestricted process. The current `planner`, `reviewer`, and `executor` IDs remain compatibility aliases until a migration is designed. No new roles below were registered by this documentation change.

| Proposed role | Inputs → inspectable output | Maximum authority | Acceptance / failure case |
| --- | --- | --- | --- |
| Product planner | Brief + explicit constraints → task DAG, unresolved questions, acceptance criteria | Read supplied project context; propose plan only | Reject cycles, missing acceptance criteria and unsupported assumptions; ambiguous high-impact scope requires a question, not fabricated requirements. |
| Contract architect | Approved requirements + dependency inventory → API/data/file-ownership contracts and decision record | Read-only inspection; propose contract revision | Referenced paths/dependencies must exist or be explicitly proposed; incompatible revisions block downstream tasks. |
| Implementation agent | Approved task + immutable contracts + owned-path baselines → staged patch and explanation | Read scoped context; write only assigned staged artifacts | No out-of-scope edits; truncated output, missing exports and stale baseline fail before commit. |
| Verification worker | Manifest + required gate policy → deterministic gate evidence | Execute allowlisted, consented checks through supervisor | Missing tooling is unknown/blocked, never passed; capture exit code and logs; cannot rewrite code to make its own check pass. |
| Repair agent | Failed gate diagnostics + staged patch + attempt budget → minimal repair proposal | Stage edits in same approved ownership scope only | Stop on repeated diagnostic fingerprint, exhausted budget or worsened required gate; no disabling tests, type checks or security policy. |
| Security reviewer | Diff + dependency/egress/process policy → located findings with severity and confidence | Read-only analysis; request a block/escalation | No secret exfiltration; demonstrate findings with location/repro guidance; cannot approve its own remediation or silently suppress findings. |
| UX/accessibility reviewer | Running preview artifacts + requested user journeys → reproducible UI findings | Read-only isolated preview interactions under explicit permission | Screenshots are not instructions; distinguish DOM/unit evidence from rendered behavior; include keyboard/focus and narrow-layout checks. |
| Documentation agent | Verified code/artifact evidence → change summary, setup/run instructions, migration notes | Stage documentation changes only | Commands and features must map to actual scripts/contracts; flag unverified setup rather than inventing successful output. |
| Release steward | Approved release manifest + gate evidence → release checklist and deployment proposal | Package staging; no publish/commit/deploy without separate grant | Recheck artifact hashes, signatures, target and rollback policy; partial deployment must remain failed/partial, not green. |

The **policy guard**, **scheduler** and **outcome evaluator** are deterministic infrastructure, not conversational agents. Keep this separation: a persuasive response cannot increase authority. Require independent review for high-risk changes; self-review may provide feedback but cannot satisfy that requirement.

Agent learning design: retain provenance (task, input hash, gate outcome, reviewer), tag observations as suggestions rather than rules, scope by project/provider/model policy, expire stale patterns and allow user deletion. Promoted patterns need evaluation against held-out fixtures. Do not implement unmeasured 1%-per-day decay, fixed scoring ratios or iterative “reflection” merely because a source class used them.

## Comprehensive feature workstreams

These stable IDs complement the phase inventory. **Designed/open** is the default; only the delivery notes below describe new source implementation.

| ID | Workstream / user-visible deliverable | Dependencies and acceptance evidence |
| --- | --- | --- |
| F01 | Workspace switcher, safe import, registered roots, project metadata and per-workspace drafts | Root authorization, import grant and migrations; reject stale IDs, path escapes and lost draft replacement. |
| F02 | Readiness/setup: native runtime, provider/model, endpoint policy, storage and required tooling | Passive checks separated from paid/network/write probes; actionable unknown/blocked states; no green fabricated health. |
| F03 | Intent library: search, review/reuse, revisions, tags, archive and export | Search/refresh/review implemented; native revisions/archive/delete/export contracts and confirmation remain open. No timestamps fabricated from random IDs. |
| F04 | Accepted plan editor: DAG, question resolution, task scope, ownership and version diff | PlanRevision + cycle/conflict validation; changing a reviewed contract invalidates approval and downstream artifacts. |
| F05 | Manifest generation: complete runnable project, per-file budget and staged output | Approved plan + gateway + workspace baseline; required entrypoints and imports verified before applying. |
| F06 | File explorer/editor/diff: generated versus existing, dirty-buffer protection, safe open-in-editor | Scoped reads, optimistic concurrency, bounded files and artifact refs; reject overwrite of user edits. |
| F07 | Dependency reconciliation/install: parser-assisted imports, package-manager detection, lockfile policy | Explicit network/script consent; deterministic report/diff; immutable install and bounded process cleanup. |
| F08 | Preview lifecycle: start/stop/readiness, owned ports, live logs, reload and crash recovery | Process supervisor + isolated origin/CSP; generated pages cannot reach privileged Tauri IPC. |
| F09 | Gate console: per-file and per-stage results, commands, failures, applicability, evidence download | Structured gate contracts; required unknown or skipped remains blocked; no inferred pass from elapsed progress. |
| F10 | Repair workspace: review patch, attempt history, diagnostic clusters, retry/rollback | Write journal + budgets + independent re-verification; detect oscillation and preserve failed attempts. |
| F11 | Agent studio: executable capability catalog, routing rationale, tasks, history, budgets and Stop | Real executor + tool policy; synthetic runtime stays disclosed; no arbitrary agent-count success criterion. |
| F12 | Provider control: secure keys, endpoint/model profiles, quotas, usage and explicit failover | Keyring/migration + gateway ownership; protect secrets in errors/exports; privacy-changing fallback needs approval. |
| F13 | Run history/reproduction: filters, pagination, immutable workspace/model/plan refs and comparison | Review rerun is implemented, not exact replay. Historical records lacking identity remain visibly non-reproducible. |
| F14 | Diagnostics/support: correlated event search, retained failures, redacted support bundles | Sequence/correlation + retention rules; never export secrets or unrelated workspace content by default. |
| F15 | Learning/provenance: useful pattern library, source evidence, pin/disable/delete and evaluation | F09 evidence + data policy; untrusted learned text cannot override system/tool/approval contracts. |
| F16 | Accessibility/layout: complete keyboard flow, landmarks, mobile drawer focus, resize/reset, large type | Existing workbench tokens/layout are implemented; rendered light/dark/narrow and assistive-tech acceptance remains open. |
| F17 | Security/resilience: sandboxing, egress scopes, race-safe filesystem, recovery, backups | Threat fixtures, fault injection, adversarial prompt/tool inputs and negative native command tests. |
| F18 | Team work: work queue, ownership leases, proposal review, handoff and conflict resolution | Single-agent path verified first; no parallel writers until F06/F10 policy and recovery work. |
| F19 | Git/delivery: reviewable changes, staged release, signed artifacts, target approvals and rollback | Explicit Git/remote/deploy consent; no automatic commits, pushes or broad staging; preserve unrelated dirty work. |
| F20 | Runtime/platform expansion: Python then other stacks, update/migration lifecycle and packaging | Per-runtime detection/build/test/preview/cleanup fixtures; portability must not weaken the supported platform's security model. |

### Dependency milestones — actual sequence overrides numeric phase labels

| Milestone | Scope | Exit gate |
| --- | --- | --- |
| M0 Trustworthy desktop substrate | F01/F02/F17, basic grants, honest state and configured gateway | Negative authorization/endpoint tests, native IPC check, explicit missing-readiness states. |
| M1 One complete generated app | F03/F04/F05/F06, sequential generation, staged review, persistence | User reviews goal/workspace, generates a full bounded Vite fixture, inspects file diff; failure cannot corrupt unrelated files. |
| M2 Executable verified app | F07/F08/F09/F10 | Consented install, boot, compile and relevant tests succeed; deliberately broken fixture fails visibly; cancellation cleans only owned resources. |
| M3 Durable operations | F13/F14, restart recovery, artifact retention and backup | Reopen app and reconstruct state/evidence without re-executing paid or destructive work; stale lease cannot write. |
| M4 Real specialized agents | F11/F12/F15 plus measured role evaluation | Agent output passes deterministic checks and permission tests; routing improves measured fixtures; budgets actually terminate work. |
| M5 Teams and delivery | F18/F19/F20 | Conflict-safe parallelism, signed artifacts, explicit deployment approval and demonstrated rollback/migration. |

### Next delivery queue (keep small even as the roadmap grows)

- **Q1 — saved-goal lifecycle:** source implemented in this continuation: separate save/list states, visible errors, stable callbacks, retry-safe IDs, stale refresh protection, searchable library. Build/behavioral evidence is recorded below; no blanket completion of F03.
- **Q2 — review intent boundary:** source implemented: Plans/History/Runner → typed `BriefReviewRequest` → Project; accept/dismiss without execution or automatic workspace restoration. Retired the untyped history event execution path. Native grants and immutable replay identity remain open.
- **Q3 — truthful agent capability surface:** source implemented: disclose current synthetic local runtime. New role contracts are designed above, not registered as working agents.
- **Q4 — passive native readiness report:** first UI slice implemented via existing `get_gateway_status`: Project explicitly requests a read-only gateway-configuration check; unconfigured/unknown/error remain visible, and a closed circuit is labeled configured but **not connection-tested**. IPC now rejects malformed scalar responses. No provider request or workspace write is triggered. **Open:** structured native report combining configuration revision, managed-workspace registration, tooling and remediation; native invocation proof, and a separately approved active probe. The current native gateway is cached in `OnceLock`, so changed environment configuration requires desktop restart.
- **Q5 — durable run identity and gate-state contract:** shared evidence presentation plus native required-gate policy are implemented / compile-checked; see the native checkpoint above. The live engine applies `pipeline/outcome.rs`, emits versioned applicability/status/receipt fields and blocks success when required checks did not run. Old records remain legacy; known failures survive degraded/malformed metadata. **Open:** behavioral acceptance, durable workspace/plan/provider/attempt identity, receipt-to-artifact binding, cancelled/interrupted distinction, event sequence, actual supervised verifier execution and SQLite reopen evidence. Existing history still represents blocked builds as failed; a distinct persisted blocked state needs a coordinated lifecycle migration. Do not enable replay from this schema-only compatibility change.
- **Q6 — approved complete manifest:** replaced the fixed two-file list with a bounded Vite manifest (`VITE_PROJECT_FILES`: 9 files) through the current `LocalGenerationManifestService` and `LocalPromptBuilderService`. Integrated into `LocalPipelineEngine::generate_project_files_with_provider_cancellable` retaining cancellation, serialization of steps and bounds limits. Passes verification on full compilation and pipeline test checks.
- **Q7 — generation ledger and reuse (ported from devcode `GenerationLedger`/`regenerationPolicy`/`generationReuse`):** per-workspace ledger at `.recode/generation-ledger.json` stores the inputs hash (goal + model + ordered file paths) and per-file sha256 of the last clean run. `pipeline/ledger.rs` decides per file between `Reuse` and `Regenerate` (disk bytes match, inputs unchanged, regular file — symlinks are never reused) and the engine splits prompts accordingly: reused files are first-class results with `Reused` status and their on-disk content; only drifted files are re-billed to the provider. Ledger IO is fail-soft (missing/corrupt/wrong-project ledger regenerates everything; bounds breaches are dropped, not fatal). The persistence gate accepts `Reused` alongside `Written` and still byte-verifies disk. A fully reused run bypasses the provider entirely (loopback test with a panicking gateway proves zero provider calls; a drifted file regenerates only itself). Verification 2026-10-09: `cargo test --workspace` 237/237 pass, `cargo check -p recode --bin recode` clean, `npm run type-check`, `npx vitest run src/lib/ipc.test.ts` 79/79, `npm run build` all pass. `GeneratedFileStatus` gained a `Reused` variant; the frontend `status` field is a plain string so no frontend type change was needed.
- **Q8 — checkpoint persistence for resumable partial runs (ported from devcode `CheckpointManager`/`ProgressTracker`):** new `pipeline/checkpoint.rs` with a per-workspace checkpoint at `.recode/generation-checkpoint.json`. Each file written during an attempt is durably recorded (via a new `generate_and_write_tracked` per-file write callback in `LocalFileGenerationService`) the moment it lands, so a run cancelled or failed mid-write no longer loses partial progress: the next run resumes files whose recorded sha256 still matches disk without re-billing the provider for them. devcode's guards are all ported: terminal checkpoints reject further progress writes, foreign attempts never clobber the active one, stale sequences are rejected, and a clean run finalizes the attempt as `Completed`, clears the checkpoint, and hands reuse back to the ledger (promoting resumed files into it — engine ledger-store filter now includes `Reused` alongside `Written`). Attempts get fresh `uuid` ids; partial runs are finalized as `Cancelled` (when `should_cancel`) or `Failed`. Resume is gated exactly like the ledger: matching inputs hash, matching disk bytes, regular files only, symlinks never resumed. All checkpoint IO is fail-soft: missing/corrupt/foreign checkpoints disable resume instead of failing the build. Verification 2026-10-09: focused slice `pipeline::checkpoint` + `engine::checkpoint_resume_tests` 9/9, full `cargo test --workspace` 246/246 pass (one initial run had 1 non-reproducible flake at 1.04s; three subsequent runs fully green), `cargo check -p recode --lib`/`--bin recode` exit 0 (warnings pre-existing), `npm run type-check` exit 0, `npx vitest run src/lib/ipc.test.ts` 79/79, `npm run build` exit 0. The engine test proves the core claim: cancelled mid-write after exactly one file, the checkpoint records that file as `Cancelled`, the resumed run bills the provider for only N−1 files and reports the resumed file as a first-class `Reused` result, then clears the checkpoint and promotes every file into the ledger.
- **Q9 — post-generation validation hard gate (ported from devcode `PostGenerationValidation`):** new `pipeline/validation.rs`. devcode's post-generation validation runs five audits, but only one is a hard failure: a malformed `package.json` must land in `failedFiles` with a parse-error reason; the other four (contract lock, behavior completeness, accessibility, UX consistency, production readiness) are warn-only devcode-DI services recode does not have and stay out of scope. Before this gate recode could accept a malformed `package.json` from the provider (the persistence gate only byte-compares), checkpoint it, promote it into the ledger, and then reuse it forever. The gate is enforced at three points: (1) provider phase — `generate_file_content` rejects malformed `package.json` content before anything is written, so prior bytes are preserved and the file never enters checkpoint or ledger (recode's stage-then-write design makes this stronger than devcode, which only marks the file failed after generation); (2) ledger reuse — `decide_regeneration` regenerates instead of reusing a ledger entry whose on-disk bytes are malformed, so legacy ledgers cannot perpetuate a bad manifest; (3) checkpoint resume — `resume_paths` applies the same guard to legacy checkpoints. Path normalization mirrors devcode (`./package.json` and `/package.json` are gated; nested `src/package.json` is not). Verification 2026-10-09: focused slice 24/24 (`pipeline::validation` 5, engine `post_generation_validation_tests` 2 — malformed package.json fails the file with the parse-error reason, siblings still generate, prior bytes preserved, no ledger stored; valid package.json still persists and promotes), `pipeline::` module 69/69, `cargo test --workspace` 253/253, `cargo check -p recode --bin recode` exit 0, `npm run type-check` exit 0, `npx vitest run src/lib/ipc.test.ts` 79/79, `npm run build` exit 0. Two pre-existing engine test fixtures updated to seed valid JSON for `package.json` (the gate correctly rejects their old non-JSON seeds).
- **Q10 — verification policy tiers (ported from devcode `VerificationPolicyService`):** new `pipeline/policy.rs`. Recode previously hardcoded all five verification gates (`persistence`, `typecheck`, `bundle`, `tests`, `startup`) as unconditionally required for every build. devcode's `VerificationPolicyService` introduces three explicit tiers (`preview-only`, `standard`, `strict`) and a risk-based policy selector (`determine_verification_policy`), along with strict-mode semantics where a skipped required gate fails the build rather than degrading it. `policy.rs` implements this pure domain logic with round-trip serde and unit tests. Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::policy::tests` passed successfully (exit 0).
- **Q11 — adaptive repair service (ported from devcode `AdaptiveRepairService`):** new `pipeline/repair.rs`. Pure domain service suggesting and applying deterministic repairs for common build failures (`missing-import`, `missing-env-config`, `missing-error-boundary`) based on compiler diagnostics and generated file content. Includes auto-apply thresholds and file patch application. Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::repair::tests` passed successfully (exit 0).
- **Q12 — generation error handler (ported from devcode `GenerationErrorHandler`):** new `pipeline/error_handler.rs`. Pure domain classifier mapping failure messages to canonical categories (`truncated`, `timeout`, `empty_response`, `parse_error`, `ai_error`) and identifying repeating failure patterns across multiple affected files. Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::error_handler::tests` passed successfully (exit 0).
- **Q13 — verification report service (ported from devcode `VerificationReportService`):** new `pipeline/report.rs`. Pure domain service generating comprehensive verification reports from build outcomes, aggregating gate results into check reports, calculating metrics (pass rate, counts), and generating actionable recommendations. Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::report::tests` passed successfully (exit 0).
- **Q14 — build outcome analyzer (ported from devcode `BuildOutcomeAnalyzer`):** new `pipeline/outcome_analyzer.rs`. Pure domain service that analyzes build histories to determine overall verdicts (`healthy`, `degraded`, `failed`), identify failure modes (`mass-compile-failure`, etc.), pinpoint root causes, and aggregate suite metrics (`success_rate`, `common_failures`, `avg_duration_ms`). Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::outcome_analyzer::tests` passed successfully (exit 0).
- **Q15 — content type validator (ported from devcode `contentTypeValidator`):** new `pipeline/content_type.rs`. Validates that AI-generated file content matches the expected file type (HTML vs CSS vs TypeScript). Prevents the "file-content misalignment" bug where the model hallucinates TypeScript source into `index.html`. Identifies detected types based on syntactic signatures, compares against the inferred path extension, and produces a critical mismatch `ReplaceWithFallback` action for severe breakages (like HTML files emitting TypeScript) supplying valid safe fallback templates. Integrated into the `generate_file_content` path in `pipeline/types.rs`. Verification 2026-10-09: `cargo test -p recode --lib -- pipeline::content_type::tests` passed (3/3), `cargo test -p recode --lib -- pipeline::` 78/78, `cargo test --workspace` all pass, `cargo check -p recode --bin recode` exit 0, `npm run type-check` exit 0, `npx vitest run src/lib/ipc.test.ts` 79/79, `npm run build` exit 0.
- **Q10–Q15 engine integration:** `BuildOutcome` struct expanded with `repair_results: Option<AdaptiveRepairResult>`, `analysis: Option<BuildOutcomeAnalysis>`, `report: Option<VerificationReport>`. `engine.rs` main outcome construction site now invokes `AdaptiveRepairService::suggest`, `BuildOutcomeAnalyzer::analyze`, and `VerificationReportService::generate_report` against live verification results and generated files. `run_verification` return type simplified from 3-tuple to 2-tuple (results + summary). All `PipelineRun` and `BuildOutcome` construction sites across `engine.rs`, `mod.rs`, `tauri_commands.rs`, and `pipeline_history.rs` updated to match the new field types. `PipelineGeneratedResult` derive changed from `Eq` to `PartialEq` (contains `f32` via `AdaptiveRepairResult`). All gates green: workspace tests pass, bin check 0, tsc 0, vitest 79/79, build 0.
- **Q16 — auxiliary file budget (ported from devcode `AuxiliaryFileBudget`):** new `pipeline/file_budget.rs`. Bounds auxiliary files (tests, stubs, router repairs) against the plan — planned files and deterministic scaffold/manifest files (package.json, tsconfig, vite config, index.html, .gitignore, README.md, public/) are never dropped; excess non-planned files beyond the 25% slack budget are trimmed. Verification 2026-10-09: `pipeline::file_budget::tests` 4/4 passed, `pipeline::` module 85/85, workspace all pass, bin 0, tsc 0, vitest 79/79, build 0.
- **Q17 — pipeline metrics collector (ported from devcode `PipelineMetricsCollector`):** new `pipeline/metrics.rs`. Records per-pipeline execution metrics (stage durations, success/failure counts, file generation counts, retry counts) and aggregates across history (total/successful/failed pipelines, average duration, per-stage success rate, cumulative file counts). Bounded history (max 1000 entries). Verification 2026-10-09: `pipeline::metrics::tests` 3/3 passed, all gates green.
- **Q18 — pattern learning service (ported from devcode `PatternLearningService`):** new `pipeline/pattern_learning.rs`. Learns from build outcomes to improve future generations — records verification failure patterns (categorized by check type with occurrence/failure counts), accumulates examples, and generates actionable insights. Bounded example list (max 5 per pattern). Verification 2026-10-09: `pipeline::pattern_learning::tests` 3/3 passed, `pipeline::` module 88/88, workspace all pass, bin 0, tsc 0, vitest 79/79, build 0.
- **Q19 — evidence collector (ported from devcode `EvidenceCollector`):** new `pipeline/evidence.rs`. Collects and organizes evidence from build pipeline runs for audit, compliance, and debugging — aggregates generated files, verification results, logs, artifacts, and metrics into typed `EvidenceItem` entries with per-type summary counts. Collections are stored by ID for later retrieval. Verification 2026-10-09: `pipeline::evidence::tests` 3/3 passed, `pipeline::` module 91/91, workspace all pass, bin 0, tsc 0, vitest 79/79, build 0.
- **Q20 — requirements traceability service (ported from devcode `RequirementsTraceabilityService`):** new `pipeline/traceability.rs`. Tracks requirements through the build pipeline from planning to verification — matches requirements to implementation and test files via keyword overlap, computes coverage metrics, identifies gaps (missing implementation, missing tests), and produces a structured `TraceabilityReport`. Verification 2026-10-09: `pipeline::traceability::tests` 3/3 passed, `pipeline::` module 98/98, all gates green.
- **Q21 — file classification utilities (ported from devcode `FileClassificationUtils`):** new `pipeline/file_classification.rs`. Pure functions for file categorization during generation: `compute_generation_waves` (test-first → contract → impl ordering), `is_testable_file` (excludes test files, configs, assets), `is_high_risk_file` (index/App/main/api/auth/services), `get_file_type_requirements` (path/language-specific generation guidance). Verification 2026-10-09: `pipeline::file_classification::tests` 4/4 passed, all gates green.
- **Q22 — pipeline adaptation policy (ported from devcode `PipelineAdaptationPolicy`):** new `pipeline/adaptation_policy.rs`. Pure policy service that gates adaptive build-pipeline actions with hard budgets: stage insertion limits, remediation insertion limits, consecutive-timeout circuit breaker, stage-retry pause-for-review threshold, speculative hit-rate scaling, and priority-ordered action filtering. Ensures adaptive behavior stays bounded and auditable. Verification 2026-10-10: `pipeline::adaptation_policy::tests` 6/6 passed, `pipeline::` module 110/110, workspace all pass, bin 0, tsc 0, vitest 79/79, build 0.
- **Q23 — build pipeline helpers (ported from devcode `BuildPipelineHelpers`):** new `pipeline/helpers.rs`. Pure utility functions for the pipeline: `derive_purpose_from_path`, `infer_language`, `estimate_file_timeout`, `calculate_dynamic_timeout`, `clean_json_response`, `estimate_file_max_tokens`, `is_truncated_response`. No regex dependency; uses extension/path heuristics for language and purpose inference. Verification 2026-10-10: `pipeline::helpers::tests` 6/6 passed, `pipeline::` module 110/110, workspace all pass, bin 0, tsc 0, vitest 79/79, build 0.
- **Q24 — file path validator (ported from devcode `filePathValidator`):** new `pipeline/file_path_validator.rs`. Pure validation of generated file paths: traversal, absolute, control chars, forbidden OS chars, markdown-fence leaks, double slashes, trailing slashes, data files used as directories; with auto-fix mode and batch validation. Verification 2026-10-10: `pipeline::file_path_validator::tests` 11/11 passed.
- **Q25 — generation prompt cache (ported from devcode `GenerationPromptCache`):** new `pipeline/prompt_cache.rs`. TTL-aware prompt-template cache with bounded size/eviction and content-hash based generation-result cache. Avoids redundant LLM calls across rebuilds. Verification 2026-10-10: `pipeline::prompt_cache::tests` 5/5 passed.
- **Q26 — generated module paths (ported from devcode `generatedModulePaths`):** new `pipeline/generated_module_paths.rs`. Pure path and identifier helpers for generated code: `is_asset_import`, `has_code_extension`, `is_test_path`, `to_module_identifier` (PascalCase, digit-prefix guard), `derive_test_path` (idempotent), `source_path_for_test`. Prevents three defect classes: asset imports scaffolded as TS modules, non-idempotent test path derivation, and unparseable identifiers from non-identifier basenames. Verification 2026-10-10: `pipeline::generated_module_paths::tests` 7/7 passed.
- **Q27 — static analysis service (ported from devcode `StaticAnalysisService`):** new `pipeline/static_analysis.rs`. Pure static analysis of generated file content: lint checks for console statements and TODO/FIXME/XXX comments, finding aggregation (summary, security score, dedup merge taking the worst of two passes, formatting for review prompts). The security-scan tool imported from `@/modules/agent-platform` is out of scope; only the pure lint/merge/score/format logic is ported. Verification 2026-10-10: `pipeline::static_analysis::tests` 6/6 passed.
- **Q22–Q27 wiring integration:** the pure domain services are now called from production code paths, not just tests:
  - `generated_module_paths::is_test_path` replaces ad-hoc test detection in `file_classification.rs:37` (compute_generation_waves).
  - `helpers::is_truncated_response` replaces the hardcoded `== Some("length")` check in `types.rs:278` (generation retry loop), now also catches `max_tokens` and `content_filter` finish reasons.
  - `file_path_validator::validate_file_path` runs before every workspace write in `types.rs:220` (generate_and_write_tracked), rejecting unsafe paths (traversal, absolute, control chars, forbidden OS chars, data-file-as-directory) and auto-fixing fixable ones (backslash normalization, leading slashes, markdown fence leaks).
  - `StaticAnalysisService::analyze_files` runs on all generated files in `engine.rs:628` and the result is surfaced as `BuildOutcome.static_analysis`, flowing through Rust serde → IPC normalization (`normalizeStaticAnalysis` in `ipc.ts`) → `RunOutcome.static_analysis` in `frontend.ts`. The frontend type `StaticAnalysisResult` includes findings, security score, files scanned, severity summary, and pass flag. All gates green: pipeline 139/139, bin 0, tsc 0, vitest 79/79, build 0.
- **Q28 — build stub generator (ported from devcode `BuildStubGenerator`):** new `pipeline/stub_generator.rs`. Pure helpers for generating minimal stub files for missing imports: `is_data_file_path`, `normalize_path`, `resolve_target_path` (relative/@ alias/absolute with extension probing, PascalCase→.tsx, bare→/index.ts), `extract_component_name` (handles index files and digit prefixes), `generate_placeholder_export` (Error class / React hook / PascalCase class / typed const variants), `build_stub` (per-language stub content for components, barrels, styles, assets). The devcode callers depend on `ConsistencyIssue`/`SymbolRegistry` DI types and are out of scope; only the pure content generation is ported. Verification 2026-10-10: `pipeline::stub_generator::tests` 14/14 passed.
- **Q29 — cross-file context builder (ported from devcode `CrossFileContextBuilder`):** new `pipeline/cross_file_context.rs`. Pure functions: `is_noise_file` (lock files, binaries, dist/build/node_modules), 4-tier `select_strategic_files` (manifests → entrypoints → domain keywords capped 3/bucket → src/lib/app core, bounded), `is_type_definition_file` (path patterns + ≥40% type export ratio), `create_file_summary` (imports/exports/types extraction, 20-line normal / 45-line contract budget), `build_cross_file_context` (dynamic budget scaled by project size, type-first selection, omission note). Wired into production: `FileGenerationPrompt` gained an optional `cross_file_context` field appended to the user message; `LocalFileGenerationService::generate_and_write_tracked` now accumulates per-file summaries as generation proceeds and passes the built context to every subsequent file; the engine seeds the accumulator with summaries of reused/checkpoint-resumed files so newly generated code stays consistent with files not regenerated this run. Verification 2026-10-10: `pipeline::cross_file_context::tests` 9/9 passed, workspace 346/346.
- **Q30 — generation error handler + auxiliary file budget (ported from devcode `GenerationErrorHandler` + `AuxiliaryFileBudget`):** new `pipeline/error_budget.rs`. Pure helpers: `classify_failure_reason` (truncated/timeout/empty_response/parse_error/ai_error from the error text), `detect_failure_pattern` (≥2 files failing with the same classified reason), `enforce_auxiliary_file_budget` (keeps the first `max(3, ceil(0.25·planned))` unplanned files, drops the rest, never drops manifest files or `public/`). Wired: engine summary now detects failure patterns in failed files' errors and appends "detected pattern: N files failed with X errors". Verification 2026-10-10: 12/12 unit tests, workspace 358/358.
- **Q31 — mental execution simulator (ported from devcode `MentalExecutionSimulator`):** new `pipeline/mental_simulator.rs`. Pre-execution symbolic dry-run over the generated module set: dependency-graph cycle detection (case-insensitive file lookup, path cycle reporting) and test-baseline validation (generated test files may only reference symbols the subject modules actually export). Types: `SimulatedModule` (path, source, optional test code, dependencies), `SimulationResult` (is_valid, graph_valid, test_baseline_valid, type_conformance_valid, cycles, diagnostics, metrics). Wired into production: the engine runs `simulate()` on every generated set after the outcome analyzer and attaches the result to `BuildOutcome.mental_simulation`; frontend gets it via `normalizeMentalSimulation` in `src/lib/ipc.ts` into `RunOutcome.mental_simulation` (`src/types/frontend.ts`). Also fixed `useAgentRuntimeSettings.loadAgentRuntimeSettings` to return a fresh copy of the defaults instead of the shared constant (vitest independence contract; caller mutation could corrupt the shared default). Verification 2026-10-10: `pipeline::mental_simulator` 8/8, workspace 366/366, tsc clean, vitest 246/246, build green.
- **Q32 — deterministic project scaffolder (ported from devcode `ProjectScaffolder` + `scaffoldPartition` + config generators):** new `pipeline/scaffolder.rs`. Configuration files (`package.json`, `tsconfig.json`, `tsconfig.app.json`, `tsconfig.node.json`, `vite.config.ts`, `.gitignore`) have exactly one correct answer per stack. The scaffolder generates them deterministically from curated, pinned manifests — no LLM involved. This eliminates ~6 provider calls per run and prevents invented dependency versions, missing peer dependencies, and wrong `moduleResolution` values (the damage class devcode's `deterministicFixes.ts` spent 1,400 lines repairing). The engine now partitions the manifest file list into scaffold (deterministic) and source (LLM) paths, writes scaffold files to disk before the provider call, and only sends source paths to the provider. Scaffold files are excluded from the generation ledger (they're deterministic, not reusable) and the checkpoint (they're written before the provider, so they always succeed). Manifest expanded from 9 to 11 files to include `tsconfig.node.json` and `.gitignore` (both essential for a complete Vite project). Verification 2026-10-10: `pipeline::scaffolder::tests` 14/14, `pipeline::engine` 8/8 (all existing tests updated for scaffold/source split), `workspace::reconciler` 11/11, full `cargo test --workspace` 419/419, `cargo check -p recode --bin recode` clean, `npm run type-check` clean, `npx vitest run src/lib/ipc.test.ts` 79/79, `npm run build` green.
- **Q33 — import path reconciliation + Vite config dependency reconciliation (ported from devcode `WorkspaceReconciliationPipeline`):** two new reconcilers in `workspace/reconciler.rs`. `DefaultImportPathReconciler` updates explicit `.ts` import paths to `.tsx` after JSX extension renaming, preventing broken imports in strict resolution mode. `DefaultViteConfigDependencyReconciler` ensures packages imported in `vite.config.ts` are listed in `package.json` devDependencies with safe version pins, preventing build failures from missing plugins. Both are now in the default `ReconciliationPipeline` alongside the existing JSX extension and index.html reconcilers. Verification 2026-10-10: `workspace::reconciler::tests` 11/11 (6 new tests for the two new reconcilers + pipeline composition), full workspace 419/419.
- **Q34 — file-type requirements wired into generation prompts:** `file_classification::get_file_type_requirements` (ported in Q21) was dead code — compiled, tested, but never called from production. Now wired into `LocalPromptBuilderService::build_file_prompts` (`prompts.rs`), injecting path/language-specific generation guidance (e.g., "Use React functional components with explicit TypeScript props interfaces" for `.tsx` files, "Validate all inputs at the boundary" for API routes) into every source-file prompt. This prevents common defect classes by giving the model file-type-specific requirements alongside the general requirements section. Also added the general requirements section ("Use strict TypeScript", "Handle errors with structured error types", "Do NOT use mock shortcuts, TODO comments") to the prompt, ported from devcode's `buildFileGenerationPrompt`. Verification 2026-10-10: `pipeline::prompts::tests` 6/6 (2 new tests verifying type-requirements injection), full workspace 421/421, all gates green.
- **Q35 — contract lock service (ported from devcode `ContractLockService`):** new `pipeline/contract_lock.rs`. Pure domain service that records and enforces cross-file contracts before and after code generation. Contracts include function signatures, schema fields, routes, error formats, and authorization rules. Generated files are validated against the lock after generation to ensure they respect agreed-upon interfaces. This prevents the common failure mode where generated files compile but violate implicit contracts (wrong parameter names, missing required fields, inconsistent error shapes). Validation is heuristic-based (substring matching for expected names in expected files), not a parser — it catches missing exports/fields/routes, not subtle type mismatches. Contracts are scoped to their declared file, so a contract for `src/services/userService.ts` only validates against that file, not every generated file. Verification 2026-10-10: `pipeline::contract_lock::tests` 7/7 (create lock, validate pass, detect missing signature/schema field/route, file-scoped validation, remove/clear locks), full workspace 428/428, all gates green.

### Required acceptance matrix and stop conditions

- **Frontend:** compile/build, component behavior, actual rendered keyboard/narrow/light/dark flows. These are separate gates; a successful bundle proves neither pixel placement nor native IPC.
- **Native contracts:** real registered command invocation; Rust serialization, path grants, error mapping, missing runtime/tool cases. Mocked `__TAURI__` payload assertions do not close this gate.
- **Persistence/recovery:** duplicate IDs, lost acknowledgment, concurrent refresh/write, process restart, full disk, interrupted migration, stale owners and missing artifacts.
- **Provider:** controlled HTTP failures, truncation, malformed output, timeout/cancel, usage reporting, redaction, explicit egress. Live provider checks are opt-in and budgeted.
- **Filesystem/process:** symlink swap, baseline conflict, partial commit, bounded output, owned port/PID cleanup, script consent and no privileged preview bridge.
- **Agent:** unsupported tool rejection, prompt injection in repository/preview/log text, budget exhaustion, attempted self-approval, conflicting file ownership, fake evidence and unauthorized retry.
- **Release:** full applicable gates, signed artifact verification, supported-platform package startup and migration/rollback fixtures. A visual approval does not authorize deployment.

Stop or escalate on scope/permission ambiguity, denied checks, stale approval, unknown required evidence, repeated repair fingerprint, conflict with user edits or exhausted resources. Preserve the failure and present the smallest next action. Do not turn “keep going” into uncontrolled background scheduling or silent infrastructure changes.

## Phase 0 — Foundation Hardening

**Goal:** make the existing recode shell reliable before adding major features.

### 0.1 IPC Typed Layer — PARTIAL
- Rust commands, TS wrappers, workspace operations and `useWorkspaceFiles` exist. Audit `main.rs` command registration against `ipc.ts` and exact payload/serde shape; close uncovered cases (including error responses and unavailable-Tauri behavior).
- Choose code generation for cross-boundary contracts only once Rust types are stable; until then pair all changes with contract tests. Remove redundant parallel implementations instead of creating new wrappers around mock services.
- **ABI correction frontend-tested, native invocation unverified:** `src/lib/ipc.ts::runPipeline` now nests the payload under `input` as `tauri_commands.rs::run_pipeline` requires and retains the response `run_id`; run-control command parameters now declare `rename_all = "snake_case"`. Audit other snake_case callers rather than treating Vitest mocks as native invocation evidence.

**Acceptance:**
- `cargo test --workspace`, `npm run test`, `npm run build` pass; contract tests verify every registered user-facing command's name, shape and error handling.

### 0.2 Frontend State Layer — PARTIAL
- Typed custom hooks exist in `src/hooks/`. Keep ephemeral view state local; add shared store only for state truly shared across views (not a blanket Zustand migration).
- Audit each mutating UI for loading, retryable error, cancellation, persisted success and stale-response handling. Error boundary catches render errors; async IPC must have explicit error state.

### 0.3 Rust Error Hygiene — PARTIAL
- `DomainError` exists, but mutex `expect`, ignored persistence errors and stringified failures in `tauri_commands.rs` remain. Remove panics on user-driven paths; preserve typed error code, actionable message and context, redact sensitive data.
- Add structured spans carrying run/project/stage IDs, then test failure, cancellation and restart paths. Do not mechanically replace test-only `unwrap()` or expose raw SQLite/HTTP details to UI.

---

## Phase 1 — Workspace + Build Pipeline (porting priority: HIGH)

Goal: the user can create a project, generate files into a workspace, and see truthful results.

### 1.1 Workspace Runtime — PARTIAL, SECURITY BLOCKER

**Completed:**
- New desktop workspaces created under Tauri `app_data_dir/projects/{id}`; all Tauri file IPC resolves the ID there rather than accepting a caller-chosen root. The lower-level `LocalWorkspaceService` still supports explicit roots for Rust library callers/tests.
- Atomic same-directory temp-file replacement (new Unix files private by default, file fsync + rename, temp cleanup on failure, existing permissions retained, internal temp artifacts hidden), with path-component validation and symlink checks; still vulnerable to concurrent parent-directory swaps and lacking directory fsync/crash recovery
- Rust file operations: `write_project_file`, `read_project_file`, `list_project_files`
- IPC commands registered in `main.rs`
- TypeScript wrappers + tests
- Regression tests for hostile workspace IDs, absolute/empty/traversal paths, pre-existing symlink escapes and valid nested files; validation IPC uses same policy as read/write
- Frontend creation/listing now passes IDs only, reloads the native managed list, and ignores stale/localStorage-supplied root paths. Duplicate workspace IDs fail instead of reusing the folder.

**Not yet implemented:**
- JSX extension reconciliation (.ts → .tsx when JSX detected)
- index.html extraction/move from public/ to root
- Source dependency reconciliation
- Import/export reconciliation
- Stub file generation for missing exports
- Required web file scaffolding (main.tsx, App.tsx, package.json, vite.config.ts)
- Dependency install with verified lockfile/runtime/installed-state cache key
- Startup script generation
- Race-resistant root and file operations, bounded and explicit-error list/read policies; reject reserved/hostile names and sensitive-file exposure across the complete command surface
- Directory fsync/crash recovery of orphan temp files and rollback/cleanup after partial workspace creation; per-workspace operation serialization

**Rust implementation remaining:**
- `src-tauri/src/workspace/reconciler.rs` — DAG of reconciliation stages
- `src-tauri/src/workspace/jsx.rs` — JSX detection + rename
- `src-tauri/src/workspace/dependencies.rs` — source dep + import/export reconciliation
- `src-tauri/src/workspace/scaffolder.rs` — required web files
- `src-tauri/src/workspace/installer.rs` — consented package install with lockfile/runtime cache key

**IPC surface remaining:**
- `reconcile_workspace(project_id) -> ReconciliationReport` with explicit changes, conflicts, and dry-run
- `install_dependencies(project_id, consent) -> InstallResult` with bounded time/output, package manager/lockfile policy and cache key including runtime/lockfile; never skip just on `package.json` hash

**Acceptance:**
- A generated Vite+React project can be created, reconciled, installed, and started from the UI
- Absolute, `..`, symlink-to-outside, ID traversal and hostile file names are rejected on every read/write/list and command path; nested valid files still work
- Installer skip requires verified lockfile + dependency state, not just a matching manifest; changed lockfile forces reinstall; failures remain visible

### 1.2 Generation Pipeline (core) — PARTIAL TWO-FILE PROVIDER PATH, NOT FULL PIPELINE

**Source:** `devcode/src/application/services/pipeline/`, `src/application/services/build-pipeline/` (plus `server/src/services/` for workspace execution)

**What to port:**
- `PipelineEngine` — stage registry, execution, pause/resume/abort, checkpoints
- `BuildGenerationExecutor` — parallel and sequential file generation
- `GenerationManifestBuilder` — batch requests, prompt building, retry on failure
- `BuildResultAggregator` — AI provider adapter, static analysis merging
- `GenerationPromptBuilder` — system/user prompt construction
- `GenerationErrorHandler` — failure classification
- `FileClassificationUtils` — ownership, wave classification
- `BuildPipelineData` / `BuildPipelineTypes` — shared types

**Rust implementation:**
- `src-tauri/src/pipeline/engine.rs` — stage execution, checkpoints (generation orchestration consolidated here after the retired `generator.rs` canned service was removed 2026-10-15)
- `src-tauri/src/pipeline/manifest.rs` — batch request building
- `src-tauri/src/pipeline/prompts.rs` — prompt construction
- `src-tauri/src/pipeline/generation_errors.rs` — extend existing failure classification

**IPC surface:**
- `run_pipeline(project_id, config) -> PipelineHandle`
- `pause_pipeline(run_id) -> ()`
- `resume_pipeline(run_id) -> ()`
- `abort_pipeline(run_id) -> ()`
- `get_pipeline_status(run_id) -> PipelineStatus`
- `get_pipeline_checkpoint(run_id) -> CheckpointData`
- `get_pipeline_result(project_id) -> BuildResult`

**Critical correctness fixes from devcode (must ship in port):**
- Truncated responses detected via `finish_reason === 'length'` and routed to one extended-budget retry (implemented for local file generation: initial per-file cap 16K, retry cap 32K; mocked gateway tests cover recovery, repeated truncation, and preservation of existing output; live provider unverified)
- Per-file `maxTokens` based on expected output size (LOC × 80, floor 2K, cap 16K)
- TODO/FIXME regex uses word boundaries (`\bTODO\b`), not substring matches
- Disk writes are awaited before verification gates run
- Store retry path upserts by path instead of appending duplicates
- Repair cluster conflicts are logged (last-writer-wins is acceptable but must be visible)

### 1.3 Verification Stages — PARTIAL (PERSISTENCE ONLY)

The default stage now checks written status, safe relative paths, regular files, and bounded on-disk content equality. The Tauri run command resolves a registered app-owned workspace before passing it to the generator; failed persistence downgrades `BuildOutcome.success`, and the gate result is returned on `PipelineRun`. Rust tests cover missing/tampered/symlinked files and mocked provider → write → verify. For a registered build, failed outcomes now produce failed history status with an inspectable result and an error UI rather than a success toast; a missing generation provider marks each requested file failed with a visible configuration error. Plan-only simulations remain distinct. App forwards only the workspace ID resolved from the native workspace list to the run action (including palette and reruns); without a selected workspace it runs a plan-only simulation. Generation currently reads `RECODE_OPENAI_BASE_URL` and `RECODE_OPENAI_MODEL` from the app process environment, not saved provider settings. Registered-run cancellation is checked before generation, between provider calls/files, and before each file write; mocked tests cover abort after planning and during a provider response. Generation stages provider content before writing any file: abort during the second request leaves both existing files intact, while an individual provider failure does not block other successful files. This is cooperative, not transactional: an in-flight blocking HTTP call can still run to its timeout, and already-written files remain after a cancellation or write error during the later write phase. The internal plan's status can still be `Completed` after build failure. Compile, boot, test, lint, browser, structural and repair stages remain absent; a live desktop/provider run is unverified. Path prechecks do not eliminate symlink-swap races.

Integration coverage now drives two requests through the real OpenAI-compatible HTTP adapter and gateway against a loopback stub, then checks generated disk bytes and the persistence gate. This does not exercise a real provider, configured desktop environment, or Tauri command invocation.

Generated non-Markdown files with a leading backtick code fence now fail before writes, preserving existing source files; Markdown README content may legitimately contain fenced examples. This only catches a common response-format error, not syntax or build failures.

**Source:** `devcode/src/application/services/pipeline/stages/`, `src/application/services/build-pipeline/`

**What to port:**
- `executeRepairPass` — LLM-powered repair from diagnostics
- `CodeVerificationService` — TypeScript compile check
- `BootVerificationService` — dev server boot check
- `TestExecutionService` — test runner
- `LintVerificationService` — lint check
- `BrowserSmokeTestService` — browser smoke
- `PersistenceVerificationService` — file persistence check
- `BuildCompletionChecks` — assemble all gates
- `BuildResultOutcome` — truthful outcome evaluation
- `SymbolVerificationService` / `SymbolRegistry` — cross-file symbol verification
- `CheckpointValidator` — skeleton/implementation completeness
- `PostGenerationVerification` — inline checks, structural audit

**Rust implementation:**
- `src-tauri/src/pipeline/verify.rs` — all verification stages
- `src-tauri/src/pipeline/repair.rs` — repair pass with diagnostics
- `src-tauri/src/pipeline/symbols.rs` — symbol registry + verification
- `src-tauri/src/pipeline/outcome.rs` — truthful outcome evaluation

**IPC surface:**
- `verify_build(project_id) -> VerificationResult`
- `repair_build(project_id, findings) -> RepairResult`
- `get_build_outcome(project_id) -> BuildOutcome`

**Acceptance:**
- A build with compile errors is reported as failed, not success
- A build with truncated files retries with extended tokens
- A build with failed files surfaces them truthfully in the UI

### 1.4 Frontend Build Views — PARTIAL UI, FILE/GATE RESULTS VISIBLE, END-TO-END ACCEPTANCE OPEN

**Source:** `devcode/src/presentation/components/build/`, `src/presentation/pages/`

**What to port:**
- BuildFailedFiles — retry failed files
- BuildResults — outcome summary with gate statuses
- ProjectPreview — preview iframe with base path rewrite
- WorkspaceTabsPanel — file tree + editor
- ErrorIntelligencePanel — error classification + auto-heal

**Acceptance:**
- User sees truthful build outcome (not false green)
- Failed files can be retried from the UI
- Verification gate statuses are visible (compile, boot, test, lint, browser, persistence)

---

## Phase 2 — Agent System

Goal: agents can be configured, routed, dispatched, and their runs are persisted.

### 2.1 Agent Registry + Execution

**Source:** `devcode/src/application/services/agents/`

**What to port:**
- Agent catalog by role
- Agent lifecycle (start, stop, pause, resume)
- Agent execution with timeout and stall detection
- Agent memory (composite key, 60s cleanup timer)

**Rust implementation:**
- `src-tauri/src/agents/registry.rs` — agent definitions
- `src-tauri/src/agents/runtime.rs` — execution state machine
- `src-tauri/src/agents/memory.rs` — LRU cache with TTL

### 2.2 Agent Routing

**Source:** `devcode/src/application/services/AgentSmartRouter.ts`

**What to port:**
- Deterministic explicit agent ID → capability/permission filtering → explainable role selection; reject unsupported work instead of dispatching to an arbitrary agent.
- Add model-assisted routing only after a labeled fixture suite demonstrates improvement over a deterministic baseline. Do not hardcode unmeasured semantic/keyword weight ratios.
- Sequential execution first; bounded DAG concurrency only after file ownership, leases, cancellation and reconciliation exist. Ensemble voting cannot override deterministic verification or approval rules.

### 2.3 Agent UI

**Source:** `devcode/src/presentation/components/agents/`

**What to port:**
- Agent catalog view
- Agent chat view
- Agent run history
- Agent configuration

---

## Phase 3 — Cognitive + Learning

Goal: agents reflect on outputs, extract patterns, and improve over time.

### 3.1 Cognitive Engine
- Evaluate whether explicit observe → plan → act → verify state adds value beyond the run coordinator before introducing a separate BDI engine.
- Expose inspectable decision summaries and unresolved questions; do not persist hidden chain-of-thought as required product data.
- Bounded reflection only after real execution/verification exists; cap iterations, time and tokens, and require measured improvement on held-out fixtures.

### 3.2 Learning System
- Evidence-backed pattern recognition with explicit samples and provenance; evaluate EMA/window policies before adopting fixed weights.
- Scoped, consented cross-agent sharing with delete/export controls.
- Retrieved pattern suggestions remain lower-trust context and cannot override approved scope, policy or tool permissions.
- Configurable expiration/decay with evaluation; no arbitrary daily rate promoted to a product requirement.

---

## Phase 4 — AI Gateway Expansion (foundation required before Phase 1 generation)

Goal: all AI calls route through a single resilient gateway.

### 4.1 Provider Registry + Gateway
- Configured provider adapters with explicit capability and health reporting; breadth follows verified demand, not an arbitrary provider count
- Circuit breaker + retry with backoff + jitter
- Failover to next healthy provider
- Token budgeting (global + per-agent)
- Prompt caching

### 4.2 Model Routing
- Per-call taskType/agentRole routing
- Deterministic model selection

---

## Phase 5 — Advanced Planning + Build Verification

### 5.1 Plan Orchestration
- Multi-phase plan generation
- Plan validation + acceptance
- Plan versioning

### 5.2 Build Verification (advanced)
- DAG orchestration
- Post-generation checks
- Plan recovery

---

## Phase 6 — Teams and Collaborative Proposals (basic approval required in Phase 0)

### 6.1 Team Intelligence
- Team roster, task assignment, file generation/review

### 6.2 Proposals + Approvals
- Improvement proposals
- Human approval workflow
- Acknowledgment-aware anomaly monitor

---

## Phase 7 — Observability + Runtime Diagnostics

### 7.1 Tracing + Telemetry
- Structured logging with trace context
- Agent execution metrics
- Pipeline stage timing
- Token usage + cost attribution

### 7.2 Runtime Diagnostics
- Console error detection (40+ patterns)
- Deterministic fixes
- LLM-powered fix fallback
- Dev server auto-restart
- Browser-side error beacon

---

## Phase 8 — Delivery + Deployment

### 8.1 Versioning
- Project version tracking
- Git integration
- Reviewable commit proposals; commit/push only on explicit user authorization, never as a side effect of a successful build

### 8.2 Deployment
- VPS deployment targets
- Release artifact generation
- Deployment verification

---

## Execution backlog — ordered by dependency and risk

All items below are **open unless explicitly marked done**. For each change: inspect source implementation and its tests, write a failing recode test for a user-observable invariant, implement through a reachable command/hook, run focused tests and the app build, then record evidence here. No automatic commits: this checkout has no git metadata. A passing stub test never closes a workflow item.

### P0 — secure and honest substrate (blocks generated code)

1. [ ] Harden `workspace/mod.rs` and `tauri_commands.rs`: race-resistant file-descriptor-relative operations (`openat2`/`RESOLVE_BENEATH` or equivalent), strict file-only reads, bounded listings and actionable errors. Test attacks on create/read/write/list/validate, concurrently swapped symlinks, absent root and valid nesting. **Reason:** generated code is untrusted; any escape is more costly than deferred features. **Implemented slices:** unsafe IDs/paths, known private names and pre-existing symlinks rejected; Tauri IPC derives registered root from app data rather than caller input; file writes use same-directory temp, fsync, rename and error cleanup. Direct reads require a regular file and cap at 1 MiB. Listing skips binary, known private/dependency paths and temporary writes; rejects missing roots, read failures, >6-depth, >2048 entries, >256 text files, >8 MiB total content or >1 MiB file rather than silently returning partial data. Newly done: race-resistant descriptor-relative file IO — `workspace/safe_io.rs` opens the root as a verified directory descriptor (device+inode re-check against symlink_metadata) and resolves every path component via rustix `openat` with `O_NOFOLLOW|O_DIRECTORY` (portable equivalent of `openat2`/`RESOLVE_BENEATH`); read/write/rename never traverse a symlink (fails with ELOOP instead), writes are temp-file → fsync → `renameat` → directory fsync, and permission preservation plus the regular-file check moved onto the descriptor path. Directory fsync/crash cleanup done: new workspace directories are fsync'd on creation, rename durability fsync's the containing directory, and failed writes clean temp artifacts. Comprehensive secret classification done: `is_private_component` now covers credential stores (`.kube`, `.docker`, `.gnupg`, `.azure`, `.databricks`, `.netlify`, `.vercel`, `.wrangler`, `.heroku`, `.terraform.d`, `.vault`), credential files (`.git-credentials`, `.gitconfig`, `.htpasswd`, `.pgpass`, `.my.cnf`, `.vault-token`, `.terraform`, `secrets.toml`, `credentials.json`), key material extensions (`.pem`, `.key`, `.p12`, `.pfx`, `.jks`, `.tfstate`, `.tfvars`, `.secret`) and SSH `id_*` keys; verified by adversarial tests for symlink-swapped path components, symlinked final targets, and non-regular file targets.  Remaining open now narrower: openat2-hardened `tauri_commands` paths and actionable-error polish. **Pipeline-side descriptor-relative IO sweep done:** all raw `std::fs` production IO in `workspace/mod.rs` (listing + orphan temp sweep), `workspace/reconciler.rs` (jsx-extension renames, index.html move, import-path writes, vite.config.ts/package.json read/write), `pipeline/engine.rs` (scaffold writes, reuse reads, excess-path removals, repair write-backs), `pipeline/verify.rs` (persistence gate read), `pipeline/validation.rs` (`is_valid_generated_file`), and `pipeline/ledger.rs` + `pipeline/checkpoint.rs` (ledger/checkpoint load, store, clear, and reuse/resume hash reads via `file_sha256`/`file_sha256_beneath` beneath a held `RootDir` descriptor) now routes through `safe_io::RootDir` with symlink refusal and size caps; new `RootDir::rename_file`/`remove_file`/`visit_entries` primitives (bounded, `getdents64`, `VisitControl` pruning) back the sweep. **Command-side TOCTOU pre-checks closed:** `safe_file_target` no longer walks a path string with `symlink_metadata` — it validates `relative` against a held `RootDir` descriptor via the new `RootDir::validate_path` (every intermediate opened with `openat(O_NOFOLLOW|O_DIRECTORY)`, final component `statat(SYMLINK_NOFOLLOW)`, missing intermediates tolerated because the actual IO walk re-checks them), and the `validate_workspace_path` IPC command inherits this by routing through it, so a concurrent symlink swap between validation and use fails the IO itself with ELOOP rather than redirecting it. **Actionable-error polish closed:** no `.expect`/`.unwrap` panics remain on user-driven IPC paths (lock expects in tauri_commands/agents/app_state/gateway/provider/model_catalog/progress_tracker/runtime_monitor now recover from poisoned locks via `PoisonError::into_inner`; time-math expects fall back to 0; the routing fallback and edge-case serde unwrap degrade gracefully). Ignored persistence (`let _ =` on plan-revision/workspace-grant/run-identity/key-status/concurrency-limit writes) now surfaces failures through capture_diagnostics instead of dropping them. IPC errors are typed in-band via `ipc_error(DomainError) -> "[code] message"` (56 former `e.to_string()` sites) plus 8 re-phrased hardcoded errors with actionable next steps; secrets never appear in messages (verified: error paths name the provider, never the key). Gates: `cargo test --workspace` exit 0 (502 lib tests), `npm run test` 247/247, `npm run build` ok. Remaining open under 0.3: structured spans carrying run/project/stage IDs.
2. [ ] Make `AppState` own the application data root and schema migration lifecycle, and resolve project IDs against registered metadata rather than any directory under `projects`. **Implemented slices:** Tauri commands derive `app_data_dir/projects` and accept IDs, not arbitrary webview roots; reads/writes/list/validation require the ID and canonical root to match a read-only SQLite registration, rejecting orphan directories and metadata redirection. Workspace creation removes its newly created directory on registration errors when it is still empty; missing DB listing does not create one. Rust unit tests and TS payload tests cover these cases. Still needed: native command integration test (mocked `__TAURI__` is not proof), explicit migration/import of existing externally rooted workspaces with user selection and persisted grant, crash/partial-schema recovery, and descriptor-relative containment against concurrent symlink swaps. Old external workspaces remain on disk but are intentionally not auto-imported or shown by the new UI.
3. [ ] Define `RunId`, project/version IDs, `RunState` (queued/running/cancelling/cancelled/failed/verified), `FileState`, `GateState` (pass/fail/skipped/unknown), timestamps and error codes in Rust; mirror with TS contract tests. **Partial:** `run_pipeline` now stores an ID-keyed `running` row and terminal `completed`/`cancelled`/`failed` state; completed full results are capped at 1 MiB and transactionally stored in a second table. Existing history rows stay readable without fabricated results, and file-backed SQLite reopen is tested. Schema migration includes compatibility with existing records; crash recovery of stale `running` rows is now done PID-aware — `PipelineRunHistoryRepository::fail_stale_running_runs` runs at process start (`AppState::open`) and terminal-fails only `running` rows whose recorded `pid` (new migrated column; recorded by `insert_run`) is NULL, legacy-missing, or no longer alive (checked via `kill(pid, 0)` / rustix `test_kill_process`), as `failed` with error "pipeline process exited before the run reached a terminal state"; idempotent, preserves terminal rows, recovered rows still refuse `finish_run` overwrites, and a concurrent live process's running row is never touched (the "does not corrupt a concurrent process" requirement of control parity); richer states, project/version identity, idempotent upserts and bounded event logs remain. Do not relabel old simulated runs as verified.
4. [ ] Consolidate pipeline ownership without deleting the live implementation: `LocalPipelineService` already calls `LocalPipelineEngine` for provider-backed generation; preserve that reachable path. Quarantine or retire the canned `LocalFileGeneratorService` in `pipeline/generator.rs` only after all consumers and negative fixtures are audited. Keep plan completion separate from file generation and required verification. **Implemented:** simulated/no-workspace runs are labeled separately; registered builds expose file and persistence-gate outcomes. **Done 2026-10-15:** consumer audit found no production callers of the canned service (only the engine's duplicate `run_with_cancellation`/`run_internal`, themselves unreferenced outside tests); retired `pipeline/generator.rs` and that duplicate orchestration — the engine now owns a single generation path (`generate_project_files_cancellable` → provider-backed flow) called by `LocalPipelineService`, so duplicate orchestration is eliminated. **Open:** complete manifest generation from planner tasks, compile/boot/test gates, approval, terminal-event consistency.
5. [ ] Finish pipeline control parity. **Implemented/verified:** `run_pipeline` consumes an ID-based `PipelineControl` with `running → pausing → paused → running` and `running/paused → cancelling`; safe points before planner steps wait on a condition variable, abort wakes them, repeated/unknown control actions return `false`. New runs reserve unique IDs in SQLite before execution; terminal state distinguishes completed/cancelled/failed in `get_pipeline_status`, with `unknown` for missing or orphaned running rows. Exact completed `PipelineRun` payloads are transactionally stored and returned by `get_pipeline_result`; legacy summary-only rows return no invented result. Repository tests cover rollback, duplicate terminal updates and DB reopen. Registered generation now checks cancellation after planning, before/after provider responses, and before each write; mocked tests cover those boundaries. **Still needed:** native Tauri integration, a single terminal event, interrupting in-flight provider/file/child work with bounded wait, transactional/inspectable partial writes on cancellation, and cancel-before-start/mid-install/duplicate-ID acceptance tests. Crash/stale-running recovery that does not corrupt a concurrent process: done — `fail_stale_running_runs` (see P0 item 3) only fails `running` rows whose recorded pid is gone, so a second live process's row survives startup recovery. A `true` pause means the request was recorded; `pausing` versus `paused` distinguishes request from effective suspension. Source: `devcode/src/application/services/pipeline/PipelineEngine.ts` (`pause`, `resume`, `abort`) and its tests.

### P1 — first real Vite/React generation (minimum replacement slice)

6. [ ] Provider execution in `gateway.rs`/`provider_settings/`: one real OpenAI-compatible adapter first with streamed/non-streamed response, timeout, cancellation, structured rate-limit/auth/network errors, capped retries with jitter, response usage and `finish_reason`. **Initial reachable slice:** env-configured non-streaming HTTP adapter behind `run_gateway_task`, HTTPS or loopback HTTP only, redirects disabled, request/response size limits, bounded timeout, basic auth/429 classification, local HTTP tests and UI-compatible chat payload. Unconfigured gateway reports `Unconfigured` and errors rather than fabricated 200. New plaintext API-key saves to SQLite are rejected and legacy key values are not loaded into the webview; old SQLite secrets remain on disk and require explicit migration. Still needed: OS keyring/encrypted storage with usable settings UI, streamed responses, cancellation, model-specific token budgets, response usage/finish-reason policy, richer structured errors and one opt-in live provider run. Capped retries with jitter: done — `RetryPolicy` (3 attempts, 250 ms base / 4 s cap) with deterministic full-jitter `jittered_delay` (no rand dependency) and `is_retryable_failure` (429/5xx/transport retry; timeouts, auth, and input errors never retry; timeout retries deliberately excluded to avoid tripled wall-clock latency), wired into the adapter's `send` loop; local HTTP mock tests cover retry-until-success, budget exhaustion, and no-retry-on-401. Do not claim 13-provider parity merely from setting rows.
7. [ ] `pipeline/manifest.rs` + `prompts.rs`: turn a concrete accepted plan into a bounded, typed file manifest with path ownership, dependency wave, expected LOC, `maxTokens = clamp(LOC × 80, 2K, 16K)` and per-run budget. Resolve explicit dependencies before parallelism; cap bytes, file count, tokens, concurrency and total wall time. Prompt contracts specify response format, project constraints and untrusted inputs. Golden tests for prompts and non-React/empty/oversized plans. **Done:** `ManifestFile` now carries `expected_loc` and `depends_on`; `GenerationManifest` carries a `ManifestBudget` (max_total_tokens/files/bytes/concurrency/wall_seconds). New `pipeline/manifest_budget.rs` (pure, 12 tests): `max_tokens_for_loc(expected_loc)` = `clamp(LOC × 80, 2K, 16K)`; `resolve_waves(files, max_waves)` does Kahn topological sort into waves — cycle or dangling edge is a hard error; `validate_budget` rejects over-file/over-token/over-byte manifests at build time. `LocalGenerationManifestService::build` now populates `expected_loc`, seeds default `depends_on` (main→App+html, App→css), validates the per-run budget, and serializes the budget. `LocalPromptBuilderService::build_file_prompts` now overrides `FileGenerationPrompt.max_tokens` with the manifest's `file_max_tokens` (LOC-driven), not the old path-keyword heuristic. Golden tests: LOC/token formula (floor/exact/cap), default dependency wiring, acyclic graph resolution, budget validation, serde round-trip, prompt uses manifest LOC. **Open:** planner-supplied `depends_on` from real plan tasks (currently canned Vite defaults), non-React manifest paths, prompt contract golden tests for empty/oversized plans, concurrency/wall-time enforcement at execution time.
8. [ ] Extend the live `LocalFileGenerationService` in `pipeline/types.rs`, reached from `pipeline/engine.rs`, rather than implementing a second generator in the canned `pipeline/generator.rs`. Preserve truncation detection, bounded retry, provider-response staging, cancellation safe points and persistence checks already present. Add approved manifest input, aggregate byte/token caps, path ownership and manifest upsert by path. Quarantine/retire the canned helper after a usage audit. Test partial failure, conflict resolution, idempotent retry, no double write, cancellation and absent provider. **Done:** `upsert_generated_file` (ported from devcode's `upsertGeneratedFile`) replaces by path in-place (position preserved) rather than appending duplicates — wired into `engine.rs` generation result merge. Aggregate byte/token caps added to `generate_and_write_tracked`: rejects upfront if total `max_tokens` > 196K or estimated bytes (tokens × 4) > 256K, so a runaway manifest cannot monopolize resources mid-generation. Tests: upsert replace/append semantics, aggregate token cap rejection. The engine already calls `generate_and_write_tracked` with manifest-derived prompts (approved manifest input) and `build_file_prompts` now uses LOC-driven `max_tokens` (P1.7). **Done 2026-10-15:** usage audit complete — the canned `pipeline/generator.rs` service and the engine's duplicate `run_with_cancellation`/`run_internal` orchestration were retired (no production callers; the live path is `LocalPipelineService` → `generate_project_files_cancellable`); 498 workspace tests green after removal. **Open:** explicit path-ownership field from plan tasks; conflict resolution tests for duplicate paths across waves.
9. [ ] `workspace/{reconciler,jsx,dependencies,scaffolder}.rs`: deterministic, ordered (classify → normalize → reconcile → scaffold → validate) transformations; recognize JSX in `.ts` without naive regex on comments/strings, update import paths, move `public/index.html` only with explicit conflict policy, reconcile imports/exports before stub creation, preserve user edits. Prefer report + explicit diff and backup over destructive silent fixes. Fixture tests from devcode's `ServerWorkspaceManager` and `WorkspaceReconciliationPipeline` cover repeat runs and conflicting exports. **Done (partial):** `reconciler.rs` now registered in `workspace/mod.rs` (was an orphan file). JSX detection rewritten: `contains_jsx` scans line-by-line skipping single-line comments (`//`), block comment regions (`/* ... */`), and string literals (`'...'`, `"..."`, `` `...` ``); detects `<` followed by an identifier start as JSX, avoiding false positives from comparison operators (`a < b`), HTML strings in template literals, and JSX-looking text in comments. 5 tests: real JSX detection, no false positive on comments (inline + multi-line + code-after-comment), strings, comparison operators, and JSX-after-string. **Open:** import path updating, `public/index.html` conflict policy, imports/exports reconciliation before stub creation, user-edit preservation, fixture tests from devcode's `ServerWorkspaceManager`. **Implemented slice:** `reconcile_workspace(project_id)` IPC command (tauri_commands.rs) runs `ReconciliationPipeline::default()` (all four reconcilers) against the registered workspace root and returns a serde `ReconciliationReport { results, modified_count, summary }`; `workspace::reconciler` now `pub`, wired in `src/lib/ipc.ts` and surfaced in ProjectView as the "Reconcile files" button (production caller).
10. [ ] `workspace/installer.rs` and `dev_server.rs`: explicit consent before package scripts/network, use detected lockfile/package manager with immutable install where possible, bounded subprocess and process-group cleanup, dependency-state hash (manifest + lock + runtime + installed state), no shell interpolation. Bind preview to loopback, allocate port safely, readiness with deadline and failure logs. Smoke test one offline fixture and one opt-in online install/start; never use a matching package hash alone as proof installed dependencies are usable. **Dev launcher hardening:** `scripts/tauri-dev-with-vite.sh` now refuses an occupied port instead of killing its owner, and on exit stops only its own Vite child; this does not establish native launch success. **Implemented slice:** `install_dependencies(project_id, consent)` IPC command (tauri_commands.rs) resolves the registered workspace via `workspace_for_id`, then `pipeline/verifiers.rs::install_step` runs a consent-gated, supervisor-bounded install (process-group cleanup, no shell interpolation); `NotSupplied`/`Denied` consent returns a note instead of spawning, missing runner never fabricates success. **Open:** lockfile/package-manager detection, immutable install, dependency-state hash, dev_server readiness.
11. [ ] `pipeline/verify.rs` + `outcome.rs`: after write barrier run persistence/structure, TypeScript compile, lint/tests and boot/browser checks as applicable. Each gate records command, elapsed time, exit status, diagnostics, applicability and evidence; distinguish unavailable tooling from pass. Set final state only by a pure outcome evaluator. Tests: compile failure cannot succeed, missing file cannot succeed, skipped required gate cannot succeed, user cancellation is not a build failure or success. **Implemented slice:** `pipeline/verifiers.rs` wires the supervised-subprocess layer into production gates — `run_verification` now spawns allowlisted commands (`npm run type-check`, `npm run build`, `npm run test:run -- --passWithNoTests`) only under `ExecutionConsent::Granted` via `run_supervised` (receipts: exit code, command, tool version); `NotSupplied`/`Denied` → gate `unknown`, never a pass; missing npm/node → `unknown`, never an executed failure; a fixed node boot-check script guards startup. `verify_build(project_id, consent)` IPC command exposes the multi-gate report standalone (fresh engine, explicit consent). Tests: compile failure cannot succeed, missing file cannot succeed, skipped required gate cannot succeed, user cancellation is not a build failure or success. **Open:** elapsed-time per gate, browser/boot checks, diagnostics capture, applicability flags.
12. [ ] `pipeline/{symbols,repair}.rs`: resolve exported symbols and cross-file references with parser-assisted checks where feasible, bounded LLM repair from structured diagnostics (max attempts/time/token budget), isolate repair drafts until verified, log collisions, never silently overwrite user edits. Add `\bTODO\b`/`\bFIXME\b` checks with policy (advisory vs blocker). Test repair failure, oscillation, cross-file contracts, truncation and post-repair regression.
13. [ ] Tauri IPC (`tauri_commands.rs` / `main.rs`) + `src/lib/ipc.ts` + `src/hooks/useRunPipeline.ts`: one command starts a persisted run, event stream includes run ID and monotonic sequence, resumable snapshot handles missed events, cancel reports terminal state, old listeners unsubscribe. Contract tests compare Rust serde payload to TS expectations; component tests show progress, failure, retry and restart recovery. **Implemented slice:** `execution_consent` threads from UI checkbox → `PipelineInput` (TS + Rust, `#[serde(default)]`) → `LocalPipelineEngine` field → `run_verification`, so every generated run is consent-scoped; three new commands `reconcile_workspace` / `install_dependencies` / `verify_build` registered in `generate_handler!` with `src/lib/ipc.ts` wrappers + normalizers and ProjectView buttons (Reconcile files / Install dependencies / Verify build) as production callers; `scripts/check-ipc-wiring.py` green (61 commands). **Open:** resumable snapshot for missed events, contract tests comparing serde payload to TS.
14. [ ] `src/views/{ProjectView,RunnerView,RunHistoryView}.tsx`: project selector, file tree/editor with dirty-file protection, build configuration, per-file progress, verification gate evidence, failed-file retry and accurate summary. Preview sandbox/proxy must not give generated code Tauri IPC or host secrets; link to local URL only after boot/readiness. Keyboard/TV/small-screen/a11y and real screenshot smoke required.

### P2 — agent, planning and provider parity (after P1 works)

15. [ ] Route goals through real planner/agent calls, not `LocalAgentRuntime` synthetic `{"status":"completed"}`. Migrate `agents.rs`, `agent_runtime.rs`, `agent_routing/` incrementally: registry roles + capabilities; deterministic explicit-ID dispatch; tool-call selection with declared schemas; semantic fallback only if measurable and explainable. Enforce timeout, cancellation, budget, tool permission, sandboxed workspace scope, stalled-run detection; persist attempts and final result. Fixture tests compare chosen route, rejected tool, failure and human handoff to devcode agent service behavior.
16. [ ] `planner.rs`/`plan_graph.rs`: approved plan with DAG dependencies, acceptance tests, contract locks, checkpoint/resume and versioned edits; reject cycles, stale revisions and unapproved high-impact actions. UI renders a diff before accepting generated changes. Do not introduce BDI/metacognition loops until base agent reliably edits/verifies real code and loop budgets are enforceable.
17. [ ] `gateway.rs` + `provider_selection.rs`: expand adapters only for actually configured providers; capability-aware routing, health, circuit breaker, bounded failover, provider-specific tokens/tools, cost accounting, offline fallback and privacy policy. Cache only safe deterministic requests with isolation by project/provider/model and invalidation; never cache secrets or mutable tool actions. Fault-injected tests for 429, 5xx, timeout, half-open transition and no healthy provider. **Implemented slice:** `model_selection.rs` persists a `SelectionState` (selected models keyed by catalog ID `{provider}/{model_id}`, disabled providers, per-model concurrency limits) in SQLite and loads it at dispatch time — `tauri_commands.rs` `dispatch`/`run_until_done` and the dynamic orchestrator — so routing honors user selection. New commands `get_selection_state` / `set_model_selected` / `set_provider_enabled` / `verify_provider_key` / `calibrate_model_concurrency` registered in `main.rs` with `src/lib/ipc.ts` wrappers; the Models panel surfaces per-provider key verification (auto-discovery on valid key), provider enable/disable, per-model selection checkboxes and concurrency calibration. Verification 2026-10-14: `cargo test` 499 lib + integration green, `tsc --noEmit` clean, vitest 247/247, `npm run build` green, `check-ipc-wiring.py` 66 commands.
18. [ ] `learning.rs` + `cognitive.rs`: consented and inspectable pattern capture, EMA/decay/dedup, cross-agent sharing boundaries, prompt-injection resistance, delete/export controls; test that patterns cannot override safety or approved plan. Metacognition max-iteration and budget guards with human-visible reasoning summaries, not unbounded autonomous mutation.
19. [ ] `agent_team.rs`, proposal/approval persistence + UI: named owner, reviewable proposal diff, permission scope, expiry/replay protection, audit log and explicit accept/reject. No unattended self-approval or team writes outside owned workspace.

### P3 — operate, extend and replace legacy

20. [ ] Unify runtime events, diagnostics, dev-server console, stage timing, token/cost attribution and trace IDs across Rust/TS. Bound storage and redact secrets; expose filtered logs, alerts and reproducible support bundle. Deterministic self-heal before provider repair; every action visible/reversible with regression gate.
21. [ ] Add language/runtime adapters progressively: Python then Go/Rust and others only when test fixtures, runtime detection, install/build/test/boot and sandbox policy exist. Do not import the entire server into the Tauri process. Browser smoke and preview base-path rewriting must verify CSP, navigation and websocket behavior without granting generated content app privileges.
22. [ ] Git/GitHub, versioning and deployment: explicit remote auth and scoped permissions, reviewable commit diff, opt-in deploy target and secret handling; build and deploy verification with rollback. Never auto-commit user files or auto-deploy on a green mock result. Reconcile updater signing and release channels separately from generated-project deployment.
23. [ ] Migration/replacement gate: imported legacy projects/history retain identity and backups; schema upgrades are reversible or restore-capable; no data loss on crash. Run `cargo test --workspace`, `npm run test`, `npm run build`, release Tauri build, live offline fixture, opt-in provider/installer E2E, accessibility and threat checks on supported Linux/TV hardware. Measure startup <2 s, IPC p95 <10 ms, workspace creation <3 s per `docs/replacement-criteria.md` on named hardware with reproducible method. Declare parity *per workflow*, not a single blanket claim.

## Explicit deferrals and design choices

- **No direct port of Fastify routes, Docker/VPS host services or cloud tenancy** into privileged desktop process. Only build a sidecar/isolated daemon if actual supported workflows require it; define auth, lifecycle and upgrade protocol first. Remote deployment is separately consented P3.
- **No blanket Zustand adoption**: existing typed hooks suffice until shared cross-view state and synchronization demand a store. Keep run truth in Rust/SQLite; UI cache is disposable.
- **No fabricated completion**: current engine, generator, agent runtime, provider stubs, planned verification files and mock-only tests are foundations or placeholders. A component changes to working only after the reachable user action and negative tests pass.
- **No broad background auto-repair** until verification evidence and user approval boundaries exist; deterministic safe repairs first. Failed validation remains failed after attempted healing.
- **Deferred feature register**: enterprise/multi-tenant auth, server-hosted collaboration, arbitrary code execution on remote hosts, all-provider breadth, speculative agent swarms and advanced deployment targets are not part of the local single-user replacement slice. Re-evaluate with source behavior and a user need, not a guessed count of classes.

## Next vertical proof

Create one Vite/React project from a goal with a configured provider, observe real file writes, install with consent, boot preview, compile/test, restart recode and see persisted truthful gate results. Repeat with an invalid path, a truncated provider response, a compile failure, cancelled run, lost event and failed install. Until all these are real, Phase 1 is **not complete** regardless of test count.
