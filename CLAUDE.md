# MESH

@docs/llm-context.md

MESH is a Wayland-only shell framework built in Rust.

It is not a compositor or window manager. It runs on top of existing Wayland compositors and provides shell UI such as panels, launchers, notifications, quick settings, overlays, widgets, and settings surfaces.

## Core idea

MESH is a platform for building desktop shell experiences with:

- Rust core runtime
- Luau scripting for extensions
- single-file UI components
- XHTML-like markup
- CSS-like styling
- package-based ecosystem
- system-wide theme inheritance
- accessibility-first component model
- localization support
- typed external configuration

## Main goals

- extensible shell platform, not hardcoded widgets
- Wayland-native shell surfaces
- downloadable widgets, services, themes, and language packs
- Material 3 inspired token-based theming
- reusable components with public settings and style hooks
- semantic metadata for accessibility and AI interaction

## Non-goals

- no custom compositor
- no custom window manager
- no full control over compositor policy
- no guaranteed identical behavior across all Wayland compositors

## Main architecture

### 1. Rust core

Responsible for:

- lifecycle
- package loading
- settings
- theming
- localization
- permissions / capabilities
- IPC / event bus
- diagnostics
- component compilation
- runtime coordination

### 2. Wayland frontend

Responsible for shell surfaces such as:

- panel
- launcher
- notification center
- control center
- overlays
- settings windows

### 3. Extension runtime

Embeds Luau and provides sandboxed host APIs.

Extensions can implement:

- widgets
- shell surfaces
- services
- themes
- language packs

Backend/service plugins must be implemented in the plugin's scripting language
through the extension runtime host API, not in Rust shell code. If a plugin
needs a new system capability, add a generic host API to the runtime and keep
the service-specific logic inside the plugin script.

Luau execution should go through a real runtime library, not hand-written
string parsing. Use `mlua` in Luau mode for script execution and treat any
custom parsing/interpreting as temporary migration code to remove, not a model
to expand.

### 4. UI component format

Single-file components inspired by Svelte.

Conceptual blocks:

- `<template>`
- `<script lang="luau">`
- `<style>`
- `<schema>`
- `<i18n>`
- `<meta>`

## Core package types

- widget
- surface
- service
- theme
- language-pack
- icon-pack

Each package should declare:

- id
- version
- compatibility
- dependencies
- capabilities
- entrypoints
- settings schema
- translations
- theme token usage

## Key concepts

### Shell surface
Top-level shell UI like panel, launcher, or notification drawer.

### Widget
Embeddable UI component inside a shell surface.

### Service
Structured provider of state/actions such as battery, media, network, notifications, AI, theme, or locale.

## Extension model

Capability-based. Packages must explicitly request access.

Examples:

- `shell.surface`
- `shell.widget`
- `service.network.read`
- `service.media.read`
- `service.notifications.post`
- `theme.read`
- `locale.read`
- `exec.launch-app`

## Theming

Token-based, shell-wide inheritance.

Suggested token groups:

- colors
- typography
- spacing
- radius
- elevation
- borders
- motion
- shadows

Components inherit tokens by default and may expose controlled style variables.

## Accessibility

Required by design.

Components should expose:

- role
- label
- description
- state
- focus metadata
- keyboard behavior
- localizable text

The semantic tree should also support automation and AI interaction.

## Localization

System-wide and package-aware:

- language packs
- package translations
- locale switching
- fallback chains
- pluralization / formatting

## Configuration

Every component should expose typed public settings so the shell can generate settings UIs and validate user configuration.

## Performance strategy

- compile components
- avoid overly dynamic runtime interpretation
- keep rendering host-driven
- isolate or throttle bad extensions
- minimize redraw and idle overhead

## Security strategy

- sandbox Luau runtime
- signed packages
- capability-based permissions
- limited host APIs
- trust levels and install-time review

## First version

Target:

- top panel
- launcher
- notification center
- quick settings
- theme engine
- package manager
- widget SDK
- service SDK
- localization support
- accessibility-aware component model

## One-line definition

MESH is a Rust-based, Wayland-native shell framework with packaged, theme-inheriting, accessibility-first components and Luau-powered extensions.
