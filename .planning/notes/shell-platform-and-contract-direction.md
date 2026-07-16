---
title: Shell platform and contract direction
date: 2026-07-16
context: Big-picture architecture exploration before documentation consolidation
---

# Shell Platform and Contract Direction

## Product identity

MESH is a shell-building platform. The Rust core supplies generic mechanisms;
the user's desktop experience is assembled from replaceable, directly editable
modules in their dotfiles.

`module.json` remains the canonical module manifest. Installed module source is
kept readable and editable rather than hidden in an immutable package store.

## Core and module boundary

The core owns module loading, contract validation, capability enforcement,
component and service execution, service transport, rendering, input, Wayland
integration, isolation, and structured diagnostics. It does not own finished
shell policy or user experiences.

Settings UI, developer tools, package tooling, panels, launchers, system
integrations, themes, and similar features are ordinary replaceable modules.
They receive no hidden privilege merely because they ship with MESH.

Each module exports one primary public unit:

- a component module exports one primary component and may contain private
  internal components;
- a service module implements one primary service interface and may contain
  private helpers;
- an interface module defines one primary contract;
- a resource module provides one primary resource family.

Components consume services through versioned contracts. Services own their
domain state. MESH transports state, methods, and events but does not accumulate
a central systemd-like global state or orchestration policy.

## Shell profiles

A shell profile is a small declarative composition document, not a process
manager. It selects root component instances, their surface placement, explicit
provider choices where selection is ambiguous, resource choices, and root
background services where needed.

Required services are normally inferred from component contracts. A sole
compatible provider may be selected automatically; multiple compatible
providers require an explicit profile choice.

Profiles can be switched live through a validated, transactional graph change:
prepare the candidate, preserve identical service instances, initialize new
services and surfaces, commit the visible switch, then remove orphaned runtime
objects. A failed candidate leaves the current profile active.

Configuration is profile-scoped. Durable service-owned data is shared across
profiles unless a service explicitly defines a different scope.

## Interface contracts

Contracts declare state, methods, events, shared types, errors, capabilities,
optional features, units, ranges, and behavioral documentation. Compatibility
is enforced at the contract boundary while new third-party interface namespaces
remain open.

Use JSON rather than introducing another native language. For substantial
interfaces, `module.json` references a separate `contract.json`; very small
contracts may remain inline. Prefer keyed objects over arrays containing
repeated `name` fields.

Contract tooling should generate strict Luau service-proxy types, provider
stubs, mock providers, documentation, compatibility reports, runtime validators,
and LSP completion/hover information.

Luau remains the native executable scripting language. TypeScript/JavaScript
and WebView-based execution remain an explicit future investigation, recorded
separately as a seed rather than part of the current contract.
