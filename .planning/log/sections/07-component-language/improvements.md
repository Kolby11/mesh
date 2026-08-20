# Section 7 — Component language: improvement audit

**Audited:** 2026-08-19
**Scope:** `mesh-core-component`, the immediate frontend compiler/host boundary,
component imports, `.mesh` block grammar, templates, props, styles, and the
runtime/tooling projections that consume the parsed contract.

This is an audit record, not a second task tracker. Open work belongs in
[`docs/BACKLOG.md`](../../../../docs/BACKLOG.md). No production code was changed
for this review.

## Logical process map

The current implementation follows this path:

```text
.mesh source
  │
  ├─ parser::extract_blocks
  │    └─ HashMap<tag, body>; top-level attributes/order/spans are discarded
  │
  ├─ script::extract_imports
  │    └─ import records + stripped script text
  │
  ├─ parser::markup
  │    ├─ brace control-flow rewritten to synthetic XML
  │    ├─ unquoted brace attributes quoted
  │    ├─ quick_xml tree construction
  │    ├─ template node lowering and component-ref ordinals
  │    └─ expressions/attributes retained as raw strings
  │
  ├─ parser::props
  │    └─ handwritten scanner → PropDef values → default validation
  │
  └─ parser::styles
       └─ lightningcss parse → simplified selectors/declarations/keyframes
       
       (the three branches are not semantically joined here)
  │
  ▼
ComponentFile { imports, props, template, script, style }
  │
  ├─ frontend compiler reads the entrypoint
  ├─ recursively reads local imports and indexes component modules
  ├─ builds a watched-path set
  ├─ validates standalone expression scope and customizable slots
  └─ lowers the AST to WidgetNode/component composition records
  │
  ├─ shell runtime resolves defaults/settings/instance props/script writes
  │    └─ publishes props.<name> and CSS prop(name) values
  ├─ style resolver consumes simplified CSS and prop variables
  ├─ module graph scans components for contributions/requirements
  └─ LSP independently locates blocks and reparses selected source regions
```

The intended ownership boundary is sound in principle: the component crate
should produce source/AST/contracts, while the compiler and runtime consume
them without making the component crate evaluate Luau, resolve modules, lay
out nodes, or paint. The main architectural weakness is that there is no
single span-preserving semantic-validation stage between the independent parse
branches and the downstream consumers. The compiler consequently re-derives
imports and script symbols with line-oriented heuristics, and runtime paths
accept or discard values that the source contract never normalized.

## Verification

- `nix develop -c cargo test -p mesh-core-component -p mesh-core-frontend -p mesh-core-frontend-host`
  passed: 58 active component tests, 64 active frontend tests, and the host
  crate's empty test target.
- `nix develop -c cargo check -p mesh-core-component -p mesh-core-frontend -p mesh-core-frontend-host`
  passed.
- The current suites do not cover module-root import containment, aliases
  colliding across nested owners, duplicate/unknown top-level blocks, malformed
  interpolation, undefined `prop()` references, or invalid higher-precedence
  prop fallback.

The delegated whole-flow and focused review workers were launched as requested,
but did not return reports before their runs stalled. The findings below were
therefore independently checked against the source and call sites rather than
attributed to an unavailable subagent result.

## Confirmed findings

### 1. Critical — local component imports can read outside the module root

`classify_import_target` accepts absolute paths, `../`, and `@src/` sources
(`crates/core/ui/component/src/parser/script.rs:134-140`). The compiler joins
those strings and reads the result immediately, with no canonical containment
check (`crates/core/frontend/compiler/src/compile.rs:195-220` and
`:277-288`). A component can therefore cause compilation to read an arbitrary
absolute `.mesh` file or a path that escapes the module through traversal; a
symlink can also redirect a path after the lexical join.

**Improve it:** make local imports a module-relative path type, reject absolute
paths and traversal, canonicalize before reading, require the resolved file to
remain under the module root/source root, and reject symlink escapes. Use the
same validated resolver for entrypoints, recursive imports, watchers, and LSP
definitions. Add tests for `../outside.mesh`, `@src/../../outside.mesh`, an
absolute path, and a symlinked file.

### 2. High — recursive local imports are indexed by a global alias, not by owner

Each source file has its own valid import namespace, but `collect_imports`
stores every parsed local component in one `HashMap<String, ComponentFile>` and
`insert_local_component` unconditionally overwrites an existing alias
(`crates/core/frontend/compiler/src/compile.rs:207-259`). Later validation looks
up a child only by `component_ref.name` (`:447-465`). If two imported components
both use `Item` for different local files, the last traversal silently supplies
the wrong AST for one owner. Local-vs-module aliases are not checked against one
another either.

**Improve it:** retain canonical target path plus owner scope in the import
graph, resolve `(owner component, alias)` to a target record, and keep module
component imports in the same collision-checked namespace. Detect cycles on the
canonical graph while retaining the authored path for diagnostics. Add a
two-branch fixture where both branches import different `Item.mesh` files.

### 3. High — top-level block extraction silently loses source and contract data

`extract_blocks` scans for known strings and stores bodies in a `HashMap`
(`crates/core/ui/component/src/parser.rs:82-133`). Unknown blocks and stray
top-level content are skipped instead of producing the declared
`ParseError::UnknownBlock`; duplicate `<template>`, `<script>`, `<style>`, or
`<props>` blocks overwrite earlier content. `<i18n>` is recognized at line 84
but is never represented in `ComponentFile` or consumed by `parse_component`
(`:43-74`). A missing `<template>` is also accepted even though the frontend
syntax guide says it is the only required block. Block attributes, including
`lang`, are ignored.

**Improve it:** parse a real top-level block sequence, reject unknown and
duplicate blocks, either implement `<i18n>` with explicit precedence or reject
it, require `<template>` for frontend components, and validate supported block
attributes (`lang="luau"`). Preserve exact source ranges and ordering in the
AST. Add fixtures for each discarded/duplicated/misdeclared case.

### 4. High — malformed interpolations can become valid literal text

`parse_inline_nodes` converts an unclosed `{...` into a `TextNode` and returns
success (`crates/core/ui/component/src/parser/markup.rs:605-647`). Empty
`{}` expressions are silently dropped, and `extract_brace_expr` can create an
empty binding (`:596-603`). This makes a typo render as literal UI or as a
missing attribute instead of failing compilation with a source-local
diagnostic. The same preprocessor family also rewrites control-flow tokens
without a final validation that its synthetic stack is empty
(`markup.rs:18-144`).

**Improve it:** use a brace-aware lexer/parser that understands escaped string
contents and reports unterminated/empty expressions, validates control-flow
nesting before XML lowering, and carries the original span into the AST. Add
negative tests for `{name`, `{}`, `onclick={}`, mismatched `{/if}`, and braces
inside quoted Luau strings.

### 5. High — the parser never joins `prop()` references to `<props>` definitions

Styles and props are parsed independently (`crates/core/ui/component/src/parser.rs:57-66`).
`standalone_prop_reference` accepts any non-nested name, including `prop()`
with an empty name (`crates/core/ui/component/src/parser/styles.rs:540-548`),
and no later component validation checks that the reference is declared, that
its declared type is legal for the CSS property, or that the embedded
`calc()`/shorthand use has a matching type. The runtime style resolver then
has only a missing variable to work with. The same gap exists for embedded
component attributes: compiler validation checks expression symbols but not
the child declaration's prop name, `expose`, or value type
(`crates/core/frontend/compiler/src/compile.rs:431-465` and `:473-508`).

**Improve it:** add a semantic component-validation pass after all blocks parse.
Build a declaration index, validate every `prop()` use and child prop
attribute, distinguish public component fields from typed props, and emit
source-located diagnostics before lowering. Keep the resulting checked
references in the compiled contract so runtime resolution cannot silently
produce an empty value. Test undefined/empty props, wrong CSS domains, unknown
child props, private props, and invalid enum/number values at use sites.

### 6. High — an invalid higher-precedence value deletes the valid lower layer

`resolved_props_json` chains default, global, instance, and per-instance values,
takes only `.last()`, and validates only that value
(`crates/core/shell/src/shell/component/runtime.rs:1408-1429`). If a per-instance
or global override is out of range or has the wrong type, the valid default or
lower setting is not recovered; the prop is omitted from the published table,
so CSS `prop(name)` and `props.name` can become empty/nil. This contradicts the
layered configuration model's fail-soft behavior.

**Improve it:** validate/normalize each layer, select the highest valid value,
retain the invalid value only in diagnostics, and make the same policy apply to
settings, embedded attributes, composition placements, and script writes. Add
tests with a valid default plus invalid global, instance, and per-instance
overrides independently.

### 7. High — structured values can be coerced into a scalar string prop

`json_to_prop_value` and its borrowed variant convert any JSON array or object
to `PropValue::String` (`crates/core/ui/component/src/lib.rs:306-328`). The
`string` validator accepts that resulting string, so at runtime a structured
value can cross a typed prop boundary as text. Some settings/composition paths
reject arrays and objects before this helper, but the shared runtime path still
allows the coercion and the behavior is inconsistent across ingress points.

**Improve it:** make JSON-to-prop conversion scalar-only and return a typed
error for arrays/objects; reserve structured values for an explicit future
`table`/`object` prop type. Add parity tests for settings, instance attributes,
composition, and script-assignment ingress.

### 8. Medium — `<props>` validation is only partial and uses ad-hoc CSS grammar

`build_prop` accepts duplicate fields with last-write-wins semantics and allows
`options`, `unit`, `min`, `max`, and `step` on unrelated types
(`crates/core/ui/component/src/parser/props.rs:243-311`). It does not reject
`min > max`, non-positive step, duplicate enum options, or invalid unit/type
combinations. The value helpers use prefix checks rather than the CSS value
grammar required by the spec; notably `is_token_value("")` returns true because
`Iterator::all` on an empty string is true (`crates/core/ui/component/src/lib.rs:452-507`).

**Improve it:** normalize `PropDef` once, reject duplicate fields and
type-inapplicable metadata, validate constraint relationships, and reuse the
CSS parser for size/color/token values. Add boundary tests for empty tokens,
malformed colors, invalid dimensions, constraint inversions, duplicate enum
options, and unit mismatches.

### 9. Medium — script/import and standalone-symbol analysis is line-oriented

`extract_imports` examines one trimmed source line at a time and strips lines
starting with `import ` (`crates/core/ui/component/src/parser/script.rs:5-37`).
The compiler separately infers public symbols with line-prefix and substring
scans (`crates/core/frontend/compiler/src/compile.rs:566-715`). These scans do
not share a Luau lexer and can diverge from real syntax around long strings,
long comments, multiline calls, escaped strings, or equivalent formatting.
They can therefore remove/retain the wrong source or report a false standalone
scope error while the real Luau runtime would parse it differently.

**Improve it:** use one Luau lexer/parser for import extraction, symbol/export
metadata, and expression identifier collection. Keep the runtime as the
authority for execution, but make the compiler consume parser-produced spans
and public-symbol metadata rather than re-parsing source text heuristically.
Add fixtures with long comments/strings, multiline imports/calls, escaped
quotes, and nested expressions.

### 10. Medium — syntax diagnostics do not retain reliable source locations

Most component errors carry only a message; template errors, props errors, and
many style errors use no span or `line: 0` (`crates/core/ui/component/src/parser.rs:24-40`,
`markup.rs:339-352`, and `styles.rs:137-176`). The block extractor also updates
its line counter using already-consumed text (`parser.rs:94-130`), which can
double-count lines after a block. LSP diagnostics consequently fall back to a
block start or line zero instead of the offending token.

**Improve it:** define a common source-span type for every AST node and parse
error, maintain offsets while scanning rather than recomputing from sliced
strings, and have LSP/CLI/compiler diagnostics render those spans. Add
multiblock line/column regression tests, including UTF-8 text and errors in
nested control-flow/style blocks.

## Recommended implementation order

1. Establish the span-preserving top-level/component parser and reject lossy
   grammar cases; make script language and `<i18n>` policy explicit.
2. Add a canonical, contained import graph keyed by owner and canonical path;
   use it for recursive compilation, cycle detection, watchers, and tooling.
3. Add the post-parse semantic pass for props, style references, child props,
   and source diagnostics; emit a checked component contract for the compiler.
4. Replace line-oriented Luau/source scans with shared lexer metadata.
5. Unify prop normalization at every runtime ingress and fix layered fallback;
   then expand tests around malformed input and cross-stage parity.

## Reusable regression matrix

| Area | Minimum regression |
| --- | --- |
| Block grammar | unknown, duplicate, missing, attribute, unsupported `lang`, and `<i18n>` cases |
| Template syntax | unclosed/empty braces, escaped quotes, malformed control-flow, nested loops/ifs |
| Import safety | traversal, absolute path, symlink, cycle, same alias under different owners |
| Prop semantics | undefined/empty `prop()`, CSS-domain mismatch, child visibility/type/value checks |
| Prop runtime | invalid override fallback, structured-to-scalar rejection, script/settings parity |
| Diagnostics | exact block/line/column for parser, style, props, and nested template failures |
| Tooling | compiler/LSP/graph all resolve the same imports, spans, and public prop contract |

