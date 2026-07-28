# Stack Research: v1.7 Modularity and Extensibility

## Existing MESH Stack

- Rust core crates already separate module loading (`mesh-core-module`), service contracts/registry (`mesh-core-service`), capabilities, config, diagnostics, scripting, frontend compiler, shell, and UI/runtime layers.
- Luau is the extension language for backend service scripts and frontend component scripts. Host APIs are generic (`mesh.exec`, config, service state/commands, events), which fits the rule that Rust should not grow service-specific branches.
- `package.json` with a top-level `mesh` section is the intended module manifest shape, while legacy manifest loaders still exist for compatibility.
- Existing docs already define the key conceptual target: a module is a package, an interface is the contract, a provider implements it, a frontend consumes it, and libraries carry reusable patterns.
- Current runtime structures have grown independently: module manifests include package identity, dependencies, capabilities, entrypoints, settings, keybinds, i18n, theme, service/provides/interface data, slots, assets, icons, and layout.

## External Patterns

- VS Code uses `package.json` as the extension manifest, with normal package metadata plus product-specific fields such as `contributes`, `activationEvents`, `capabilities`, dependencies, and extension kind. This validates MESH's choice to keep standard package metadata separate from MESH-specific behavior.
- VS Code contribution points are static manifest declarations. Commands, configuration, keybindings, menus, themes, languages, grammars, icons, and other capabilities are registered through one contribution namespace rather than scattered top-level concepts.
- VS Code activation events show the value of lazy runtime entrypoints: extension code starts in response to declared events instead of every extension becoming always-on.
- WebExtensions use a mandatory `manifest.json` that declares metadata and functionality, while permissions are explicit capability requests. MDN separates install-time, optional, host, and API permissions, which is useful vocabulary for MESH capability design.
- GNOME Shell extensions are simple directory packages with required metadata and code entrypoints, but they are tightly coupled to shell internals. MESH should avoid that failure mode by keeping stable contracts and diagnostics ahead of raw internal access.
- Kubernetes custom resources show a useful distinction between extending a platform API and using the platform as arbitrary application storage. MESH interfaces should extend runtime contracts, not become a dumping ground for module-private data.

## Stack Additions Needed

- A canonical manifest-schema layer in `mesh-core-module` that describes all MESH-specific sections under one normalized `mesh` namespace.
- A migration/compatibility diagnostic layer that reports legacy manifest shapes, deprecated terms, and lossless/lossy conversions.
- A contribution index that treats UI entrypoints, slots, libraries, resources, keybinds, settings, and interface/provider declarations as typed contributions.
- Contract-driven validation between interface declarations, provider implementations, frontend dependencies, capability requests, and generated scripting/LSP metadata.
- No new foundational runtime language is needed. The milestone should consolidate Rust schemas, diagnostics, docs, examples, and Luau-facing metadata around the existing stack.

## Sources

- VS Code Extension Manifest: https://code.visualstudio.com/api/references/extension-manifest
- VS Code Contribution Points: https://code.visualstudio.com/api/references/contribution-points
- VS Code Activation Events: https://code.visualstudio.com/api/references/activation-events
- MDN WebExtensions manifest: https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json
- MDN WebExtensions permissions: https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/permissions
- GNOME Shell extension anatomy: https://gjs.guide/extensions/overview/anatomy.html
- Kubernetes custom resources: https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/custom-resources/
