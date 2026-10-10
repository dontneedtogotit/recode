# Replacement Criteria

This checklist defines when the Tauri 2 + Rust `recode` rewrite can replace the current Vite/React + Node/Fastify stack.

## Parity checklist

- [ ] Local workspaces can be created and opened from the UI
- [ ] Provider settings persist across app restarts
- [ ] Agent planner can create multi-step plans from a goal
- [ ] Circuit breaker and self-heal loops remain operational
- [ ] Dev server TCP allocation and readiness await behave as before

## Performance

- [ ] App startup is under 2s on Omarchy hardware
- [ ] IPC p95 latency is under 10ms
- [ ] Workspace creation completes in under 3s end-to-end

## Stability

- [ ] No regression in frontend test count
- [ ] Rust lib and integration tests remain green
- [ ] Tauri release binary compiles cleanly

## Deprecation gate

All 6 parity conditions must be true before the legacy stack is deprecated.
