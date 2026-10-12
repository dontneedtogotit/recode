# LLM Provider and Model Configuration Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Implement professional AI provider and model configuration management, replacing reliance on static environment variables with validated, runtime-configurable settings.

**Approach:**
1.  Enhance `ProviderSettings` data model and repository for secure API key injection (Runtime-only, in-memory).
2.  Implement a `ProviderRegistry` or `ProviderService` to select the active provider/model dynamically based on enabled settings.
3.  Inject selected provider/model into the `LocalPipelineEngine` generation flow.
4.  Expose configuration capabilities in the frontend via IPC.

---

### Task 1: Enhance Provider Settings Model
**Objective:** Support model-specific settings and secure runtime key injection.
- Modify: `src-tauri/src/provider_settings/mod.rs`
- Add optional `model` field to `ProviderSettings`.
- Implement safe in-memory-only key management that does NOT persist keys to the SQLite database.

### Task 2: Provider Service Implementation
**Objective:** Add logic for selecting the active provider.
- Create: `src-tauri/src/pipeline/provider_orchestrator.rs`
- Implement `ProviderOrchestrator` to:
    - Load provider configurations from the repository.
    - Validate enabled status.
    - Provide an `active_provider(&self) -> Result<ProviderConfig>` wrapper to use in the pipeline engine.

### Task 3: Engine Integration
**Objective:** Wire dynamic provider config into the generation pipeline.
- Modify: `src-tauri/src/pipeline/engine.rs`
- Update `generate_project_files_cancellable` to use `ProviderOrchestrator` instead of `RECODE_OPENAI_*` env vars.

### Task 4: Frontend IPC and UI Setup
**Objective:** Allow configuration of providers in the desktop app.
- Modify: `src/lib/ipc.ts`
- Add commands: `saveProviderSettings`, `listProviders`.
- (If time/scope permits): Sketch basic UI for enabling/setting model.

---

**Verification:**
- Run `cargo test -p recode --lib pipeline::` ensuring no regressions in the pipeline.
- Update `engine.rs` tests to mock provider configurations properly without relying on env vars.
- Verify `cargo check` passes.

---

**Status:** Awaiting user approval of this plan.
