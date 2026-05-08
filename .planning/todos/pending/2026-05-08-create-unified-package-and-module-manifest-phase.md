---
created: 2026-05-08T10:02:41.147Z
title: Create unified package and module manifest phase
area: planning
files:
  - .planning/ROADMAP.md
  - config/package.json
  - config/modules/@mesh/*/package.json
  - modules/frontend/*/module.json
  - modules/icon-packs/*/module.json
  - modules/backend/*/package.json
  - modules/backend/*/module.json
---

## Problem

MESH currently has multiple manifest shapes and names across installed package configuration, frontend modules, backend modules, and icon packs. The user wants a separate future phase to improve the overall package/module manifest structure, module management, icon pack installation, and interface declarations.

The desired direction is a unified interface at least at the manifest-schema level: each layer should have a similar package.json or module.json structure for versions, dependencies, and type-specific configuration. The phase should explicitly reconsider whether the canonical manifest name should be `module.json`, `package.json`, or layer-specific names, and whether every layer should use the same name.

## Solution

Create a separate roadmap phase that designs and implements a shared manifest contract across frontend modules, backend modules, icon packs, and installed module configuration. The phase should cover:

- Canonical manifest naming: `module.json` vs `package.json` vs layer-specific names.
- Shared fields for versions, dependencies, capabilities/interfaces, and metadata.
- Type-specific config sections keyed by module layer/type rather than entirely separate manifest schemas.
- Module management rules for installation, dependency resolution, and compatibility checks.
- Icon pack installation and declaration flow.
- Interface declaration format so modules can state what they provide and consume consistently.
- Migration or compatibility behavior for existing manifests under `config/modules`, `modules/frontend`, `modules/backend`, and `modules/icon-packs`.
