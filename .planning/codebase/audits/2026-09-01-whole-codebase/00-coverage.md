# Whole-codebase audit coverage

**Audit date:** 2026-09-01
**Scope:** MESH source, tests, benchmarks, manifests, shipped module data,
build scripts, authoring tools, and the contract documentation needed to audit
their seams. This is a read-only audit; no source fixes are made.

## Inventory method

The inventory was built before section review with `rg --files -uu`, excluding
`.git/`, build output, `.planning/` working/history material, the audit output
itself, and binary assets. `cargo metadata --no-deps --format-version 1` was
also run and recorded 30 workspace packages. The resulting in-scope inventory
contains **648 files**, with **648 assigned** and **0 unassigned**.

Assignment is by primary ownership. Neighboring packages may inspect an
assigned file when tracing a seam, but that does not create a second owner.
The following exhaustive source-root assignment is the inventory manifest:

| Section | Assigned roots / files | Count |
| --- | --- | ---: |
| 01 | `crates/core/foundation/{capability,config,debug,diagnostics}/`; `config/settings.json`; `docs/spec/08-settings.md` | 14 |
| 02 | `crates/core/extension/module/`; `config/modules/`; all shipped `module.json` files except interface manifests; `docs/spec/{README,01-module-system,02-installation}.md` | 72 |
| 03 | `crates/core/extension/service/`; `modules/interfaces/*/module.json` | 9 |
| 04 | `crates/core/foundation/theme/`; `modules/themes/`; `docs/spec/04-styling.md` | 18 |
| 05 | `crates/core/foundation/locale/`; shipped module `config/i18n/` catalogs; `docs/spec/07-i18n.md` | 11 |
| 06 | `crates/core/foundation/resources/`; `crates/core/ui/icon/`; `modules/icon-packs/`; `docs/spec/{05-icons,06-fonts}.md` | 18 |
| 07 | `crates/core/ui/component/`; `docs/frontend/mesh-syntax.md`; `docs/spec/03-components.md` | 13 |
| 08 | `crates/core/ui/elements/`; `docs/frontend/elements.md`; `docs/spec/09-accessibility.md` | 51 |
| 09 | `crates/core/ui/{animation,interaction}/`; `docs/spec/10-keyboard.md` | 18 |
| 10 | `crates/core/frontend/{abi,compiler,host,shell-adapter}/`; `crates/core/ui/expression/`; shipped frontend `.mesh` files; `modules/frontend/shared/`; `docs/frontend/renderer-contract.md` | 68 |
| 11 | `crates/core/runtime/`; backend module `src/main.luau` files | 59 |
| 12 | `crates/core/frontend/render/`; `config/performance-baseline.tsv`; `tools/check-performance`; renderer/performance docs | 57 |
| 13 | `crates/core/{surface-config,surface-policy}/` | 4 |
| 14 | `crates/core/{platform/wayland,presentation}/` | 26 |
| 15 | `crates/core/shell/`; `config/module.json`; `modules/compositions/`; architecture and crate-boundary docs | 134 |
| 16 | `crates/tools/`; editor integrations; scripts; build/workflow/config files; general project and tool docs; automation/MCP specs | 76 |

The section reports repeat their assigned roots and record the exact files
inspected or any follow-up needed. The implementation inventory was checked
against the same `rg --files -uu` exclusions and the Cargo package list; no
unassigned in-scope path remains.

## Excluded files and why

- `.git/**`: repository internals.
- `target/**` and other build output: generated artifacts, not source ownership.
- `.planning/archive/**`: explicitly frozen by project policy; never current.
- `.planning/**` outside the required contract/history reads and this audit:
  planning history, prototypes, spikes, todos, and notes are evidence rather
  than current implementation. Existing section reports and
  `performance-log.md` were read as historical evidence and are not rewritten.
- Binary assets (`png`, `jpg`, `jpeg`, `gif`, `webp`, `ico`, `bmp`, `ttf`,
  `otf`, `woff`, `woff2`): asset bytes are not source logic; their manifests,
  mappings, and consumers are assigned above.

## Section status

| Section | Assigned | Inspected | Excluded from section | Needs review |
| --- | ---: | ---: | --- | --- |
| 01 | 14 | 14 | none beyond global exclusions | none |
| 02 | 72 | 72 | none beyond global exclusions | none |
| 03 | 9 | 9 | none beyond global exclusions | none |
| 04 | 18 | 18 | none beyond global exclusions | none |
| 05 | 11 | 11 | none beyond global exclusions | none |
| 06 | 18 | 18 | binary font bytes excluded; see global exclusions | none |
| 07 | 13 | 13 | none beyond global exclusions | none |
| 08 | 51 | 51 | none beyond global exclusions | none |
| 09 | 18 | 18 | none beyond global exclusions | none |
| 10 | 68 | 68 | none beyond global exclusions | none |
| 11 | 59 | 59 | none beyond global exclusions | none |
| 12 | 57 | 57 | none beyond global exclusions | none |
| 13 | 4 | 4 | none beyond global exclusions | none |
| 14 | 26 | 26 | none beyond global exclusions | none |
| 15 | 134 | 134 | none beyond global exclusions | none |
| 16 | 76 | reused from historical planning report; not re-inspected in this pass | none beyond global exclusions | none |
