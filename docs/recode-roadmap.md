# recode roadmap

This roadmap is intentionally structured around architecture first. The goal is not feature volume; it is a clean boundary between desktop shell, domain, persistence, provider gateway, and frontend so new capabilities compose predictably.

## current state
- Tauri 2 desktop shell with IPC commands wired in `main.rs`.
- Rust backend modules for workspace, pipeline, planner, agent runtime, persistence, gateway/provider, self-heal, monitoring, quality, updater, benchmarks.
- React frontend functional, but largely monolithic in `App.tsx` with significant inline styling and view branching.
- Persistence opens `recode.db` from multiple command sites instead of a shared app-state entrypoint.

## phase 1 — architecture cleanup
**Goal:** stabilize internal boundaries without changing visible behavior.

1. **unified error model**
   - establish one backend error type for command boundaries.
   - keep `DomainError` for internal domain flows, but convert once at the IPC layer.

2. **centralized persistence bootstrapping**
   - replace repeated `open_recode_db(...)` calls with a single app-state connection source.
   - ensure schema creation is idempotent and happens in one place.

3. **provider/gateway type consolidation**
   - remove duplicate `ProviderAdapter` / `GatewayService` definitions.
   - keep gateway as the single source of truth for provider routing and circuit breaker behavior.

4. **frontend decomposition**
   - split `App.tsx` into view components and shared UI primitives.
   - extract inline styles into a small, stable styling layer instead of ad-hoc objects.

## phase 2 — delivery hardening
**Goal:** make the desktop app reliable enough for repeated real use.

1. **startup and IPC telemetry**
   - wire startup benchmark and IPC benchmark results into persisted events.
   - expose benchmark history in the frontend instead of transient state only.

2. **update and security posture**
   - harden update verification inputs and signature paths.
   - document required manifest shape and fallback behavior when checks fail.

3. **runtime diagnostics surface**
   - unify monitor events, self-heal actions, and diagnostics into one backend stream.
   - reduce the number of separate persistence tables without losing queryability.

## phase 3 — product expansion
**Goal:** use the cleaned boundaries to ship higher-leverage user-facing capabilities.

1. **multi-workspace workflows**
   - workspace selection, recent list, and status summary instead of manual root entry.

2. **plan execution feedback**
   - richer plan step status, retry controls, and failure explanation.
   - connect plan failures to self-heal actions deterministically.

3. **quality baseline enforcement**
   - persist baselines by project root.
   - block or warn on quality regressions before execution continues.

## implementation rule
Each phase should land as small, reviewable changes:
- backend trait/domain changes first,
- IPC command updates next,
- frontend wiring last,
- tests alongside implementation, not after.
