# MESH Frontend Compiler

`mesh-core-frontend` owns frontend module compilation, source-to-widget
lowering, UI tag normalization, and accessibility defaults.

It produces `WidgetNode` trees and does not paint or present them. Painting
lives in `mesh-core-render` at `crates/core/frontend/render`; window and
layer-shell presentation lives in `mesh-core-presentation`.
