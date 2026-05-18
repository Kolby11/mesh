# Pitfalls Research: v1.7 Modularity and Extensibility

## Risks

| Risk | Why It Matters | Prevention |
|------|----------------|------------|
| Terminology churn without code alignment | Docs could rename concepts while structs and diagnostics keep the old mental model. | Start with an inventory and require every term to map to a runtime structure or compatibility alias. |
| Breaking existing manifests | v1.1 backend package graph and v1.6 keybind declarations are already useful. | Keep compatibility loaders and add explicit migration diagnostics before removing anything. |
| Overloading capabilities | Capabilities could drift into interface identity, provider selection, or dependency declarations. | Keep capabilities as host powers; use interfaces/providers/dependencies for contract relationships. |
| Contributions become ad hoc again | New extension points could bypass the installed graph. | Route contributions through typed manifest sections and contribution indexing. |
| Manifest schema grows too broad | A giant schema can become hard to validate and document. | Group by purpose: identity, dependencies, capabilities, entrypoints, contributions, contracts, compatibility. |
| Core regains service-specific branches | Convenience APIs can undermine the extensibility value. | Require proof modules to add behavior through interfaces/providers/libraries only. |
| Migration diagnostics are too noisy | Authors may ignore warnings if compatibility mode produces broad noise. | Make diagnostics specific, actionable, and scoped to the loaded module. |

## Warning Signs

- A new module type cannot be explained as package identity plus typed contributions.
- A frontend imports a backend provider ID instead of an interface contract.
- A backend requests consumer capabilities such as service-specific read/control powers just because it implements an interface.
- A library module grants host power without the consumer declaring capabilities.
- Docs and runtime examples use different names for package, module, provider, interface, contribution, or capability.
- The proof path requires adding an audio/network/power-specific Rust branch.

## Phase Coverage

- Vocabulary drift belongs in Phase 37.
- Manifest/schema compatibility belongs in Phase 38.
- Contribution and interface/provider routing belongs in Phase 39.
- Compatibility/migration diagnostics belong in Phase 40.
- Service-specific core branch prevention and author proof belong in Phase 41.
