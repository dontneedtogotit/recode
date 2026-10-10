# ADR-001: Platform Selection — Tauri 2 + Rust Backend

## Status

Proposed

## Context

CodeForge currently ships as a Vite/React frontend plus a separate Node/Fastify backend. We want a native Omarchy Linux desktop application with improved performance, smaller footprint, memory safety, and tighter OS integration.

## Decision

Adopt **Tauri 2** as the desktop shell with a **Rust** backend (`src-tauri`) and preserve the existing **React 19 + TypeScript + Vite** frontend (`src/`).

## Rationale

- **Native Linux desktop:** Tauri uses the system webview, producing a smaller, faster binary than Electron.
- **Rust backend:** Provides memory safety, async performance via Tokio, and a mature ecosystem for IPC, process management, and SQLite.
- **Frontend reuse:** Retains React/TS/Vite investment; UI work continues without a full rewrite.
- **Packaging:** Tauri supports AppImage out of the box and can be packaged for pacman/Flatpak, matching Omarchy targets.
- **Security:** Tauri’s capability system restricts IPC and filesystem access by default.

## Consequences

- Requires Rust toolchain and Tauri CLI for development.
- IPC boundaries replace in-process frontend/backend coupling.
- Native build environment requirements on Omarchy.
