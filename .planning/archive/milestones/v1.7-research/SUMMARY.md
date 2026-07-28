# Research Summary: v1.7 Modularity and Extensibility

## Recommendation

Make v1.7 a consolidation milestone: define one module/package/contribution model, align runtime validation and diagnostics with that model, and prove that third-party extension paths stay generic.

## Stack Additions

- Canonical `mesh` manifest schema documentation and normalized Rust model updates.
- Typed contribution indexing for UI entrypoints, libraries, resources, settings, keybinds, interface declarations, and provider declarations.
- Compatibility diagnostics for legacy manifests and milestone-grown fields.
- Contract/capability validation that keeps service behavior out of Rust core.

## Feature Table Stakes

- One vocabulary: package/module, interface, provider, frontend, library, resource pack, contribution, capability, dependency.
- One manifest model under `package.json.mesh`.
- Explicit dependency/capability/provider rules.
- Migration support for current backend graph and keybind declarations.
- Author docs and proof module path.

## Watch Out For

- Do not rename docs without updating diagnostics and structs.
- Do not break existing manifests silently.
- Do not let capabilities become identity or provider selection.
- Do not let new extension points bypass the installed graph.
- Do not add service-specific Rust APIs during the proof.

## Useful External Lessons

- VS Code keeps extension behavior in manifest fields such as `contributes`, `activationEvents`, and `capabilities`, while preserving standard package metadata.
- WebExtensions separate manifest functionality from explicit permissions, including install-time and optional permission concepts.
- GNOME Shell extensions show that simple packaging is useful, but direct shell-internal coupling makes compatibility fragile.
- Kubernetes custom resources reinforce that extensibility contracts should extend the platform API, not store arbitrary private application state.

## Sources

- VS Code Extension Manifest: https://code.visualstudio.com/api/references/extension-manifest
- VS Code Contribution Points: https://code.visualstudio.com/api/references/contribution-points
- VS Code Activation Events: https://code.visualstudio.com/api/references/activation-events
- MDN WebExtensions manifest: https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json
- MDN WebExtensions permissions: https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/manifest.json/permissions
- GNOME Shell extension anatomy: https://gjs.guide/extensions/overview/anatomy.html
- Kubernetes custom resources: https://kubernetes.io/docs/concepts/extend-kubernetes/api-extension/custom-resources/
