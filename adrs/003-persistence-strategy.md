# ADR-003: Persistence Strategy — SQLite via tauri-plugin-sql

## Status

Proposed

## Context

Current CodeForge uses IndexedDB for browser persistence and files for workspace state. We need durable, queryable, embedded persistence for a desktop app.

## Decision

Use **SQLite** as the primary persistence layer, accessed via **tauri-plugin-sql** from the frontend and **rusqlite** from the Rust backend.

## Rationale

- **Embedded:** No separate database server; matches desktop deployment.
- **ACID:** Durable writes for agent state, provider settings, and workspace metadata.
- **Queryable:** Enables analytics and migrations without custom tooling.
- **Shared access:** Both frontend and backend can operate on the same database file with connection management.

## Consequences

- Requires schema migrations for each release.
- Connection pooling/lifetime must be managed across Tauri command invocations.
