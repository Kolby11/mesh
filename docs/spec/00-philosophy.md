# 00 — Platform Philosophy

> Part of the [MESH Specification](README.md). This is the canonical source
> for product principles, vocabulary, and the core/module ownership boundary.

**Status: accepted direction, confirmed 2026-09-05.** These principles govern
design and review. They are not a claim that every requirement is implemented;
the detailed specifications distinguish shipped behavior from targets. A
scripting-language replacement remains undecided (§7).

## 1. What MESH is

MESH is a Rust-based, Wayland-native platform for building desktop shell
experiences from editable modules. It runs above an existing compositor and
provides panels, launchers, notifications, quick settings, overlays, widgets,
and settings surfaces. A shipped shell family is one composition of modules.

MESH is not a compositor, window manager, general process supervisor, or fixed
desktop environment. It does not control compositor policy or promise identical
behavior across compositors. Its component language uses XHTML-like markup and
CSS-like styling; resemblance to the web does not imply browser compatibility.

## 2. Core owns platform invariants; modules own experiences

The ownership test is: **does this behavior need one authoritative implementation
to keep the platform consistent, or does it express a particular desktop
experience?** The goal is a clear boundary, not the smallest possible Rust core.

Core necessarily defines some policy: settings precedence, permission checks,
focus behavior, and transaction semantics must be consistent. Calling core
merely a wiring layer obscures these responsibilities.

| Rust core owns | Modules own |
| --- | --- |
| Module discovery, validation, dependency resolution, activation, and lifecycle | Installable desktop features and their declared dependencies |
| Typed service transport, contract validation, and permission enforcement | Domain integrations and behavior behind those contracts |
| Compilation, script execution, layout, rendering, input, and Wayland presentation | Component structure, visual design, and interaction flows |
| Settings validation, precedence, persistence, and authoritative mutations | Settings pages, controls, and the meaning of module preferences |
| Scoped persistent storage primitives | Saved domain records and how to interpret them |
| Package/profile transaction machinery and resource resolution | Package UI, shell composition choices, themes, and resource packs |
| Runtime inspection, structured diagnostics, and profiling APIs | Devtools and inspector interfaces |
| Element semantics, accessibility, focus, and localization machinery | Labels, translations, domain meaning, and accessible composition |

For example, a settings transaction still makes sense when someone replaces the
shipped settings UI. A hardcoded Bluetooth settings tab does not. Audio device
normalization belongs in an audio provider; rendering a generic slider belongs
in core; choosing the audio control's layout belongs in a component.

### Built-in services and ordinary UI

The browser analogy is useful for ownership: the platform supplies batteries
such as storage and inspection, while UI is authored using its normal component
language. MESH settings, devtools, and package UIs are ordinary `.mesh`
components using capability-gated built-in services. They do not become native
elements or privileged module kinds because of their purpose.

Core owns the authoritative settings, storage, package/profile, resource, and
inspection mechanisms. A replaceable settings UI does not imply a replaceable
third-party settings engine. Persistence details remain behind core APIs so
clients do not depend on files or duplicate validation. Alternative storage
engines are not a promised module extension point.

Shipped `@mesh` modules have no hidden privilege. Access follows resolved
capability grants, never a module-name check. Built-in core services are explicit
platform authorities; desktop service implementations remain module-owned.

## 3. Modules, components, and composition

| Term | Meaning |
| --- | --- |
| Module | An installable, versioned unit with canonical `module.json`; all MESH behavior is under `mesh`. |
| Module kind | Its packaging role: `frontend`, `backend`, `interface`, `component`, `composition`, `library`, `theme`, `icon-pack`, `font-pack`, or `language-pack`. |
| Element | A built-in UI primitive with runtime behavior, styling hooks, and semantic guarantees (§4). |
| Component | A reusable user-authored `.mesh` unit composing elements and other components. |
| Widget | A component used inside another shell surface; not a separate execution model. |
| Surface | A top-level presentation container for a component instance. |
| Interface | A named, versioned, declarative contract for service operations/state/events or UI extension points. |
| Provider | An implementation of a service interface, supplied by a backend module or an explicitly built-in core service. |
| Contribution | An explicitly declared addition to the graph, such as a UI entry, resource, or extension-point implementation. |
| Library | Importable script helpers; importing code grants no host capabilities. |
| Composition | A module selecting roots, providers, resources, and slot arrangements; it binds but neither implements services nor grants privileges. |
| Profile | A composition instance plus user deltas, or a hand-built set of composition decisions. |

There is one UI component model. A UI module has one primary/default public
component, may contain private components, and may explicitly declare additional
public contributions. A default export is not a prohibition on named public
entries. Private files do not become a public API merely by being installed.

`frontend` supplies default surface placement and installation behavior;
`component` supplies reusable UI with no `mesh.surface` block of its own. Both
use the same component semantics. **Target:** a profile can explicitly mount
either kind as a root, providing placement where the module has none. Mounting
or embedding never grants capabilities. The current root activation path still
requires `frontend`; direct frontend installation adds its default instance,
while installing a component only makes it available. See [01](01-module-system.md).

Profiles select root instances, ambiguous providers, resources, and scoped
configuration. Dependencies and sole compatible providers may be resolved
automatically, but permissions may not. Installed availability, active
composition, preference overrides, and observed health remain distinct state.
Durable service data is shared across profiles unless its contract says
otherwise; profile switching preserves compatible live service instances.

## 4. Elements enforce shared standards

Core elements embody stable interaction and semantic standards so module authors
do not have to remember or independently recreate essential behavior. Building
a shell should not mean inventing a different keyboard, focus, editing, or
accessibility model in every module. The shared standards support visual and
behavioral customization without making essential platform behavior optional.

A new element is justified by reusable input, layout, rendering, or accessibility
behavior that requires runtime support or one consistent implementation.
Reusability or a recurring visual appearance alone is not sufficient: components
already provide reuse.

- `button` owns activation, focus, disabled behavior, and accessible semantics.
- `input` owns editing, selection, clipboard/input-method integration, and value
  semantics.
- `slider` owns range handling, pointer/keyboard input, and accessible values.
- `scroll-area` owns scrolling, clipping, and layout/input integration.
- Menu focus traversal and dismissal can be core behavior; menu content and
  visual composition belong in components.

A volume control composes a slider, button, icon, and text and connects them to
an audio interface. Core does not need a `volume-control` element. Settings
rows, media cards, and launcher results follow the same rule.

Core supplies sensible semantics and diagnostics. Authors still supply correct
labels, translations, relationships, and domain meaning. Themes customize
appearance; they do not replace the underlying interaction contract. Accessibility
and localization are required foundations. The semantic tree also supports
automation through separately authorized APIs.

## 5. Explicit contracts and permissions

Frontends consume service interfaces rather than concrete backend identities.
Backends expose stable domain state: they normalize provider-specific data and
own domain decisions. Frontends derive presentation such as labels and icon
choices. Core validates and transports that state without inventing domain or
display fields. Generic, contract-declared transport behavior remains core work.

**Target:** runnable services require explicit typed contracts, inline for small
cases or in a separate interface module. Inline declarations reduce packaging
ceremony; runtime inference is not an alternative authoring model. Existing
permissive paths are migration gaps, documented in [01 §4](01-module-system.md#4-interfaces).

Host/OS powers are closed and core-defined. **Target:** interface authors can
define namespaced service-operation permissions without a Rust release, with
validated ownership, privilege classification, explicit consumer requests,
and approval. Such a permission authorizes a declared service operation; it
cannot create host powers, impersonate a built-in permission, or enlarge the
provider's host grants. The current capability catalog is closed for both host
and service permissions; [01 §7](01-module-system.md#7-capabilities--security)
records that implementation boundary.

## 6. User preferences and runtime control

Defaults are declared once; user overrides are sparse. Settings, generated UI,
and style/script projections share typed declarations. Configuration is distinct
from module-owned persistent data and temporary runtime state.

Scripts have final control over effective component props, subject to value
validation and core invariants. They can honor, adapt, or temporarily override a
user preference, but must be able to inspect that preference independently of
their own assignment. The props contract includes effective-value provenance
and access to underlying user layers; see [03 §4](03-components.md#4-precedence--one-specificity-ladder)
for its API and implementation status.

A temporary `props.width` assignment does not persist a new user preference.
Persistent changes use the settings service. A storage write updates the
module's own saved data, not another module's configuration. Neither a script
override nor a user preference bypasses permission checks, declared surface
constraints, or required element behavior.

Themes provide inherited semantic tokens, with Material 3 inspiration rather
than a fixed desktop appearance. Icon/font/language packs resolve logical names
through ordered fallback chains; one active theme provides coherence. Modules
can expose controlled style hooks and localized typed preferences.

## 7. Runtime and language direction

The shell and its module execution run within one process. Module/component
environments keep variables private by default. Cross-boundary access requires
both an explicit public API and an explicit import, reference, subscription, or
interface use. Importing a module does not expose its private environment or
transfer its capabilities.

Shared VMs are compatible with this model; a VM per module or an OS process per
module is not required. Ordinary script failures must be attributed, logged,
and shown locally with a bounded error state. They must not crash the shell or
blank unrelated UI. Core owns execution budgets, cleanup, diagnostics, and
recovery. Environment isolation, authorization, and resource accounting are
separate responsibilities; sharing a VM does not prove all three. Detailed
runtime coverage remains in [01 §8](01-module-system.md#8-module-lifecycle).

One process does not guarantee survival of every native crash or process-wide
memory failure. Likewise, running above Wayland does not by itself establish
an unspoofable permission UI or compositor-wide overlay protection. Consent
authority stays in core; any stronger presentation guarantee needs a supported
platform mechanism and its own documented evidence.

**Current implementation:** Luau through `mlua`, with ordinary language syntax
and sandboxed host APIs. Prefer standard language constructs over magic globals
or special parsing. **Undecided:** whether TypeScript/JavaScript should replace
or supplement Luau, motivated by access to its library ecosystem. No migration,
browser/Node compatibility, WASM tier, or native Rust module ABI is committed.
Language-independent contracts and ownership rules must survive that decision.

## 8. Applying the philosophy in reviews

Use the ownership test in §2 and element test in §4 before proposing a new core
feature. Trace contracts and permissions across boundaries; separate domain
state from presentation, preferences from runtime state, and availability from
activation. Runtime changes should be transactional where state crosses a
commit boundary, bounded, recoverable, and observable. Compilation, incremental
work, and low idle/redraw overhead are design priorities; performance claims
still require measurements.

This chapter owns principles. Detailed specs own schemas and shipped/target
status; [architecture](../architecture/overview.md) and
[crate boundaries](../crate-boundaries.md) describe implementation ownership.
Audit prompts link here instead of maintaining another set of principles.
Historical section reports remain evidence, not current authority. Open work
belongs only in [the backlog](../BACKLOG.md).
