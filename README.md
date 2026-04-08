# MESH

MESH is a Wayland-native shell framework built in Rust. It runs on top of an existing Wayland compositor and provides the shell layer of the desktop: panels, launchers, notifications, quick settings, overlays, widgets, and settings surfaces.

MESH is not a compositor and not a window manager. The goal is to build a polished, extensible shell platform rather than replace the full compositor stack.

## What MESH is

MESH is designed as a desktop shell platform with a strong extension model.

It combines:

- a Rust core for performance, safety, and system integration
- Luau for scripting extensions and package logic
- single-file UI components
- package-based distribution for widgets, services, themes, and translations
- a shared theme system with Material 3 inspired design tokens
- built-in accessibility and localization support
- external customization through typed settings and controlled style hooks

The project aims to make third-party components feel native to the shell rather than visually or architecturally disconnected.

## What MESH is not

MESH does not try to:

- implement its own compositor
- implement its own window manager
- own core compositor policy such as window focus or workspace rules
- guarantee identical behavior on every Wayland compositor

Instead, it acts as a Wayland shell client and system UX layer.

## Main goals

The goals of MESH are:

- create a modular shell platform instead of a hardcoded widget set
- make widgets, services, and shell surfaces installable through packages
- ensure all components inherit a shared shell-wide theme
- make components configurable from outside through typed settings
- provide a clean API for custom services and custom UI
- enforce accessibility and localization from the beginning
- support semantic metadata that can be used by screen readers, tooling, and AI systems

## Core architecture

MESH is split into four main parts.

### Rust core

The Rust core owns the system-level parts of the shell:

- lifecycle and runtime coordination
- package loading
- settings storage and validation
- theme engine
- localization engine
- permission and capability model
- IPC and event bus
- diagnostics and logging
- component compilation and runtime integration

This layer should remain strongly typed and authoritative.

### Wayland frontend layer

This layer is responsible for displaying shell UI on Wayland. It includes surfaces such as:

- top panel
- app launcher
- notification center
- quick settings
- overlays and popups
- settings windows
- optional lock screen support later, where available

MESH should treat these as shell surfaces hosted on Wayland, not as compositor-owned primitives.

### Extension runtime

MESH embeds Luau as a sandboxed runtime for extensions.

Extensions can provide:

- widgets
- shell surfaces
- services
- themes
- language packs

The extension runtime should expose stable host APIs instead of raw unrestricted access to the system.

### UI component layer

UI should use a single-file component model inspired by Svelte.

A component can contain:

- markup
- Luau logic
- styles
- public settings schema
- translations
- metadata

Conceptually, component files may use blocks like:

- `<template>`
- `<script lang="luau">`
- `<style>`
- `<schema>`
- `<i18n>`
- `<meta>`

This keeps structure, logic, and styling close together while still supporting validation and compilation.

## Core model

MESH should clearly separate three concepts.

### Shell surface

A shell surface is a top-level piece of UI shown independently on screen.

Examples:

- panel
- launcher
- notification drawer
- control center
- overlay

### Widget

A widget is an embeddable component placed inside a shell surface.

Examples:

- clock
- battery indicator
- weather card
- media controls
- network row

### Service

A service is a structured provider of state and actions.

Examples:

- battery
- media
- network
- notifications
- theme
- locale
- AI assistant

This separation keeps the ecosystem understandable and easier to maintain.

## Package system

Packages are the main delivery format for MESH.

Supported package types should include:

- widgets
- surfaces
- services
- themes
- language packs
- icon packs

Each package should declare:

- package id
- version
- compatibility range
- dependencies
- requested capabilities
- entrypoints
- settings schema
- translations
- theme token usage

This gives the shell enough information to install, validate, configure, and secure packages consistently.

## Extension model

The extension model should be capability-based.

A package must explicitly request what it wants to do. The host can then allow, deny, or review these permissions.

Examples include:

- `shell.surface`
- `shell.widget`
- `service.network.read`
- `service.network.control`
- `service.media.read`
- `service.notifications.post`
- `theme.read`
- `locale.read`
- `exec.launch-app`

This keeps third-party extensions powerful but bounded.

## Theming

Theming is one of the key selling points of MESH.

The shell should use a token-based theme system inspired by Material 3 style ideas. Components should inherit system-wide tokens by default so the shell feels visually unified.

Recommended token groups include:

- colors
- typography
- spacing
- corner radius
- elevation
- borders
- motion
- shadows

Components may expose extra style hooks, but they should still align with the shared token system.

## Accessibility

Accessibility should be required by design, not added later.

Components should expose:

- accessible name
- description
- role
- state
- focus metadata
- keyboard interaction
- tooltip or help text
- localizable strings

The same semantic layer can also support automation and AI integrations in a structured way.

## Localization

Localization should be system-wide and package-aware.

MESH should support:

- global language selection
- package translations
- language packs
- locale fallback chains
- pluralization and formatting
- runtime locale switching where practical

No user-facing package should be forced to hardcode strings.

## Configuration

Each component should expose a typed settings schema so that MESH can validate configuration and generate settings UIs automatically.

This should make it easy for users to customize packages without editing internal implementation details.

## Performance and security

Because shell UI is always present, performance matters.

MESH should:

- compile components instead of interpreting everything dynamically
- keep rendering and animation host-driven
- minimize redraw and idle overhead
- isolate or limit misbehaving extensions

Security matters as soon as third-party packages exist.

MESH should support:

- sandboxed Luau execution
- signed packages
- capability-based permissions
- restricted host APIs
- install-time review and trust levels

## First version

A realistic first version of MESH should focus on:

- top panel
- app launcher
- notification center
- quick settings
- theme engine
- package manager
- widget SDK
- service SDK
- localization support
- accessibility-aware component model

This is enough to prove the platform without expanding into compositor-level scope.

## Summary

MESH is a Rust-based shell platform for Wayland. It is focused on extensibility, theme consistency, accessibility, and package-driven customization.

Its main idea is simple: build a shell platform with clear architecture and strong extension contracts, so surfaces, widgets, services, themes, and translations all work together as one coherent system.