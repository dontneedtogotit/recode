# ADR-002: IPC Strategy — Tauri Commands + Events

## Status

Proposed

## Context

The rewrite needs typed, auditable communication between the webview and the Rust backend. We must avoid ad-hoc HTTP or shared memory schemes.

## Decision

Use **Tauri 2 commands** for request/response IPC and **Tauri events** for one-way backend-to-frontend notifications.

## Rationale

- **Type safety:** Tauri generates TypeScript bindings from Rust command signatures, reducing contract drift.
- **Auditable surface:** Every IPC command is declared in Rust, making security review explicit.
- **Performance:** IPC serialization is lightweight compared to HTTP.
- **Events:** Decouple long-running operations (workspace creation, self-heal) from request/response flow.

## Consequences

- Frontend must use Tauri’s `invoke()` API rather than `fetch()` for backend calls.
- Rust commands must be registered and exported.
- Event handling must manage ordering and idempotency.
