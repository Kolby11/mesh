# Tauri + TypeScript Pivot

MESH is pivoting away from `.mesh` and Luau for community plugin authoring.

The new split is:

- Rust core owns plugin discovery, capabilities, lifecycle, bindable state wiring, shell orchestration, and system integration.
- Backend plugins are pure TypeScript and communicate with core over a typed host protocol.
- Frontend plugins are Tauri-hosted components, with Svelte + TypeScript as the preferred authoring stack.

## Core responsibilities

The core remains responsible for:

- capability enforcement
- bindable value registration and subscription
- cross-plugin event routing
- shell surface lifecycle
- settings, theme, locale, diagnostics
- exposing stable host APIs to plugin runtimes

## Plugin responsibilities

Backend plugins should:

- register bindable values
- update service state
- invoke core actions through the host bridge
- publish domain events

Frontend plugins should:

- register a Tauri/Svelte entrypoint
- subscribe to bindable values
- render shell UI
- invoke core actions in response to user interaction

## Runtime contract

The initial host protocol lives in `crates/mesh-runtime/src/protocol.rs` and is mirrored by `sdk/typescript/mesh-core-api/src/index.ts`.

The important primitives are:

- `register_bindable`
- `update_bindable`
- `subscribe_bindable`
- `invoke_core`
- `emit_event`
- `register_frontend`
- `register_backend`

This gives us a single contract for both TypeScript backends and Tauri frontends while keeping the core authoritative.
